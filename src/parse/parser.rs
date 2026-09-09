use crate::ast::expr::Expression;
use crate::ast::structure::{
    Attribute, Block, Body, DynamicBlock, PostconditionBlock, PreconditionBlock, ValidationBlock,
};
use crate::ast::user_func::{FunctionBlock, FunctionParam};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::error::HclError;
use crate::lex::lexer::Lexer;
use crate::lex::token::{Token, TokenKind};
use crate::parse::type_expr::TypeExprParser;
use crate::span::Span;
use std::collections::HashMap;

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

    // Check for mixed tabs and spaces in individual lines
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

    // Identify non-empty lines and check cross-line consistency
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

    // Find common prefix among all non-empty lines
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
            // Empty or whitespace-only lines: strip up to common_prefix, or leave blank if shorter
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

/// The HCL Parser.
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Option<Token>,
    next_tok: Option<Token>,
    diags: Diagnostics,
    leading_comments: Vec<String>,
    trailing_comment: Option<String>,
    last_line: usize,
}

impl<'a> Parser<'a> {
    /// Create a new Parser for the given input string.
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self::new_with_file(input, None)
    }

    /// Create a new Parser for the given input string with an optional source file origin.
    ///
    /// # Arguments
    /// * `input` - The source text to parse.
    /// * `file` - Optional source file identifier.
    #[must_use]
    pub fn new_with_file(input: &'a str, file: Option<std::sync::Arc<str>>) -> Self {
        let mut lexer = Lexer::new_with_file(input, file);
        let mut diags = Diagnostics::new();
        let mut leading_comments = Vec::new();
        let mut trailing_comment = None;
        let mut last_line = 0;
        let current = Self::advance_lexer(
            &mut lexer,
            &mut diags,
            &mut leading_comments,
            &mut trailing_comment,
            &mut last_line,
        );
        let next_tok = Self::advance_lexer(
            &mut lexer,
            &mut diags,
            &mut leading_comments,
            &mut trailing_comment,
            &mut last_line,
        );
        Self {
            lexer,
            current,
            next_tok,
            diags,
            leading_comments,
            trailing_comment,
            last_line,
        }
    }

    /// Return the current diagnostics.
    #[must_use]
    pub fn errors(&self) -> &Diagnostics {
        &self.diags
    }

    fn advance_lexer(
        lexer: &mut Lexer<'a>,
        diags: &mut Diagnostics,
        leading_comments: &mut Vec<String>,
        trailing_comment: &mut Option<String>,
        last_line: &mut usize,
    ) -> Option<Token> {
        loop {
            match lexer.next() {
                Some(Ok(token)) => {
                    if token.kind == TokenKind::Whitespace {
                        continue;
                    }
                    if token.kind == TokenKind::Comment || token.kind == TokenKind::InlineComment {
                        if token.span.start_line == *last_line {
                            *trailing_comment = Some(token.text);
                        } else {
                            leading_comments.push(token.text);
                        }
                        continue;
                    }
                    if token.kind == TokenKind::Newline {
                        *last_line = 0;
                    } else {
                        *last_line = token.span.end_line;
                    }
                    return Some(token);
                }
                Some(Err(err)) => {
                    #[rustfmt::skip]
                    diags.push(Diagnostic::error( "Lexical Error".to_string(), format!("Unrecognized token '{}'", err.text), err.span, ));
                }
                None => return None,
            }
        }
    }

    /// Advance the parser, returning the consumed token.
    fn advance(&mut self) -> Option<Token> {
        let current = self.current.take();
        self.current = self.next_tok.take();
        self.next_tok = Self::advance_lexer(
            &mut self.lexer,
            &mut self.diags,
            &mut self.leading_comments,
            &mut self.trailing_comment,
            &mut self.last_line,
        );
        current
    }

    /// Check the current token's kind without advancing.
    fn peek_kind(&self) -> Option<TokenKind> {
        self.current.as_ref().map(|t| t.kind.clone())
    }

    /// Parse a top-level Body.
    pub fn parse_body(&mut self) -> Body {
        self.parse_body_inner(None)
    }

    fn parse_body_inner(&mut self, end_token: Option<&TokenKind>) -> Body {
        let mut attributes: HashMap<String, Attribute> = HashMap::new();
        let mut blocks = Vec::new();
        let mut functions = Vec::new();
        let mut dynamic_blocks = Vec::new();
        let mut validations = Vec::new();
        let mut preconditions = Vec::new();
        let mut postconditions = Vec::new();
        let mut first_span: Option<Span> = None;
        let mut last_span: Option<Span> = None;

        loop {
            // Skip leading newlines.
            while self.peek_kind() == Some(TokenKind::Newline) {
                self.advance();
            }

            if self.peek_kind().as_ref() == end_token {
                break;
            }

            // A body item must start with an identifier.
            let Some(ident_tok) = self.advance() else {
                break;
            };
            if ident_tok.kind != TokenKind::Ident {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Argument or block definition required".to_string(), "An argument or block definition is required here.".to_string(), ident_tok.span, ));
                self.recover_after_body_item();
                continue;
            }

            if first_span.is_none() {
                first_span = Some(ident_tok.span.clone());
            }
            last_span = Some(ident_tok.span.clone());

            if ident_tok.text == "function" {
                if let Some(func_block) = self.finish_parsing_function(&ident_tok) {
                    last_span = Some(func_block.span.clone());
                    functions.push(func_block);
                    continue;
                }
                continue;
            }

            if ident_tok.text == "dynamic" && self.peek_kind() != Some(TokenKind::Assign) {
                if let Some(dyn_block) = self.finish_parsing_dynamic_block(&ident_tok) {
                    last_span = Some(dyn_block.span.clone());
                    dynamic_blocks.push(dyn_block);
                    continue;
                }
                continue;
            }

            if ident_tok.text == "validation" && self.peek_kind() == Some(TokenKind::OBrace) {
                if let Some(val_block) = self.finish_parsing_validation(&ident_tok) {
                    last_span = Some(val_block.span.clone());
                    validations.push(val_block);
                    continue;
                }
                continue;
            }

            if ident_tok.text == "precondition" && self.peek_kind() == Some(TokenKind::OBrace) {
                if let Some(pre_block) = self.finish_parsing_precondition(&ident_tok) {
                    last_span = Some(pre_block.span.clone());
                    preconditions.push(pre_block);
                    continue;
                }
                continue;
            }

            if ident_tok.text == "postcondition" && self.peek_kind() == Some(TokenKind::OBrace) {
                if let Some(post_block) = self.finish_parsing_postcondition(&ident_tok) {
                    last_span = Some(post_block.span.clone());
                    postconditions.push(post_block);
                    continue;
                }
                continue;
            }

            let item_leading_comments = std::mem::take(&mut self.leading_comments);
            let next = self.peek_kind();
            match next {
                Some(TokenKind::Assign) => {
                    if let Some(attr) =
                        self.finish_parsing_attribute(ident_tok, item_leading_comments)
                    {
                        last_span = Some(attr.span.clone());
                        if let Some(existing) = attributes.get(&attr.name) {
                            #[rustfmt::skip]
                            self.diags.push(Diagnostic::error( "Attribute redefined".to_string(), format!( "The argument {:?} was already set at line {}, col {}. Each argument may be set only once.", attr.name, existing.name_span.start_line, existing.name_span.start_col ), attr.name_span, ));
                        } else {
                            attributes.insert(attr.name.clone(), attr);
                        }
                    }
                }
                Some(TokenKind::String | TokenKind::OBrace | TokenKind::Ident) => {
                    if let Some(block) = self.finish_parsing_block(ident_tok, item_leading_comments)
                    {
                        last_span = Some(block.span.clone());
                        blocks.push(block);
                    }
                }
                _ => {
                    #[rustfmt::skip]
                    self.diags.push(Diagnostic::error( "Argument or block definition required".to_string(), "An argument or block definition is required here. To set an argument, use the equals sign \"=\" to introduce the argument value.".to_string(), ident_tok.span, ));
                    self.recover_after_body_item();
                }
            }
        }

        let mut span = Span::new(0, 0, 0, 0, 0, 0);
        if let (Some(first), Some(last)) = (first_span, last_span) {
            span = Span::new(
                first.start_byte,
                last.end_byte,
                first.start_line,
                first.start_col,
                last.end_line,
                last.end_col,
            );
        }

        Body {
            attributes,
            blocks,
            functions,
            dynamic_blocks,
            validations,
            preconditions,
            postconditions,
            span,
        }
    }

    fn finish_parsing_attribute(
        &mut self,
        ident: Token,
        leading_comments: Vec<String>,
    ) -> Option<Attribute> {
        let assign_tok = self.advance()?; // We know it's Assign

        let Some(expr) = self.parse_expression() else {
            self.recover_after_body_item();
            return None;
        };

        let end_span = expr.span();
        let trailing_comment = self.trailing_comment.take();

        // Must end with a newline or EOF.
        let next = self.peek_kind();
        match next {
            Some(TokenKind::Newline) | None => {
                if next.is_some() {
                    self.advance(); // eat newline
                }
            }
            Some(TokenKind::Comma) => {
                let bad = self.advance()?;
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Unexpected comma after argument".to_string(), "Argument definitions must be separated by newlines, not commas. An argument definition must end with a newline.".to_string(), bad.span, ));
                self.recover_after_body_item();
            }
            Some(_) => {
                let bad = self.current.as_ref()?;
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Missing newline after argument".to_string(), "An argument definition must end with a newline.".to_string(), bad.span.clone(), ));
                self.recover_after_body_item();
            }
        }

        Some(Attribute {
            name: ident.text,
            expr,
            span: Span::new(
                ident.span.start_byte,
                end_span.end_byte,
                ident.span.start_line,
                ident.span.start_col,
                end_span.end_line,
                end_span.end_col,
            ),
            name_span: ident.span,
            equals_span: assign_tok.span,
            leading_comments,
            trailing_comment,
        })
    }

    fn finish_parsing_function(&mut self, ident: &Token) -> Option<FunctionBlock> {
        // Syntax: function "name" { params(a, b type) ... return_type = type ... result = expr }
        // Wait, standard HCL function syntax is:
        // function "name" {
        //   params = [a, b]
        //   result = a + b
        // }
        // Wait, what is the exact syntax for user-defined functions in HCL?
        // Ah, looking at the @TODO_PLAN.md: `function "name" { ... }`.
        // Let's implement it as a block and then extract the `params`, `result`, `return_type` attributes.
        // Wait, `finish_parsing_function` shouldn't be a separate block if we just parse it as a block?
        // But the prompt says "Allow the parser to recognize `function "name" { ... }` block definitions."
        // We can parse it as a block and then convert it, or parse it directly.
        // Let's parse it similarly to a block, but we require a string label for the name.
        let name_tok = match self.advance() {
            Some(t) if t.kind == TokenKind::String => t,
            Some(bad) => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Invalid function definition".to_string(), "A function definition requires a quoted string name.".to_string(), bad.span, ));
                self.recover_after_body_item();
                return None;
            }
            None => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Unexpected EOF".to_string(), "Expected function name.".to_string(), ident.span.clone(), ));
                return None;
            }
        };

        // Name is quoted, strip quotes
        let name = name_tok.text.trim_matches('"').to_string();

        let _open_brace = match self.advance() {
            Some(t) if t.kind == TokenKind::OBrace => t,
            Some(bad) => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Invalid function definition".to_string(), "Expected opening brace `{` after function name.".to_string(), bad.span, ));
                self.recover_after_body_item();
                return None;
            }
            None => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Unexpected EOF".to_string(), "Expected opening brace `{` after function name.".to_string(), ident.span.clone(), ));
                return None;
            }
        };

        let body = self.parse_body_inner(Some(&TokenKind::CBrace));

        let close_brace_span = if let Some(t) = self.advance() {
            t.span
        } else {
            #[rustfmt::skip]
            self.diags.push(Diagnostic::error( "Unexpected EOF".to_string(), "Expected closing brace `}` for function.".to_string(), ident.span.clone(), ));
            return None;
        };

        let next = self.peek_kind();
        match next {
            Some(TokenKind::Newline) | None => {
                if next.is_some() {
                    self.advance(); // eat newline
                }
            }
            _ => {
                let bad = self.current.as_ref()?;
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Missing newline after function definition".to_string(), "A function definition must end with a newline.".to_string(), bad.span.clone(), ));
                self.recover_after_body_item();
            }
        }

        // Now extract params and result from the body
        let mut params = Vec::new();
        // Wait, how are params defined? In Packer/Terraform, it's typically `params = [ ... ]`
        // We will just use `params` as a list of strings/variables?
        // Let's check `FunctionParam` struct. It has `name: String`, `type_expr: Option<TypeExpr>`.
        // Terraform 1.8 provider functions:
        // function "name" {
        //   params = [
        //     // wait, how to type them?
        //   ]
        // }
        // Wait, since we are designing the AST and parser:
        // Let's look at standard `params = [a, b]` (list of identifiers or strings?).
        // Actually, Packer functions:
        // function "foo" {
        //   params = [a, b]
        //   result = a + b
        // }
        // Let's extract `params` from attributes
        if let Some(p_attr) = body.attributes.get("params")
            && let Expression::Tuple(elements, _) = &p_attr.expr
        {
            for el in elements {
                if let Expression::Variable(var_name, span) = el {
                    params.push(FunctionParam {
                        name: var_name.clone(),
                        type_expr: None,
                        span: span.clone(),
                    });
                } else if let Expression::String(str_val, span) = el {
                    params.push(FunctionParam {
                        name: str_val.clone(),
                        type_expr: None,
                        span: span.clone(),
                    });
                } else {
                    // ignore or error?
                }
            }
        }

        let result_expr = if let Some(r_attr) = body.attributes.get("result") {
            r_attr.expr.clone()
        } else {
            // Error, missing result
            #[rustfmt::skip]
            self.diags.push(Diagnostic::error( "Missing result".to_string(), "A function definition must have a `result = ...` attribute.".to_string(), close_brace_span.clone(), ));
            Expression::Null(close_brace_span.clone())
        };

        // Note: We could parse `return_type` attribute if it exists
        let mut return_type = None;
        if let Some(rt_attr) = body.attributes.get("return_type")
            && let Expression::Variable(ty_name, _) = &rt_attr.expr
        {
            let mut p = TypeExprParser::new(ty_name);
            if let Ok(te) = p.parse() {
                return_type = Some(te);
            }
        }

        let mut variadic_param = None;
        if let Some(vp_attr) = body.attributes.get("variadic_param") {
            match &vp_attr.expr {
                Expression::Variable(var_name, span) => {
                    variadic_param = Some(FunctionParam {
                        name: var_name.clone(),
                        type_expr: None,
                        span: span.clone(),
                    });
                }
                Expression::String(str_val, span) => {
                    variadic_param = Some(FunctionParam {
                        name: str_val.clone(),
                        type_expr: None,
                        span: span.clone(),
                    });
                }
                _ => {}
            }
        }

        for block in &body.blocks {
            if block.block_type == "variadic_param" {
                let var_name = if let Some(n_attr) = block.body.attributes.get("name") {
                    match &n_attr.expr {
                        Expression::Variable(s, _) | Expression::String(s, _) => Some(s.clone()),
                        _ => None,
                    }
                } else if !block.labels.is_empty() {
                    Some(block.labels[0].clone())
                } else {
                    None
                };

                let type_expr = if let Some(t_attr) = block.body.attributes.get("type") {
                    if let Expression::Variable(s, _) | Expression::String(s, _) = &t_attr.expr {
                        TypeExprParser::new(s).parse().ok()
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(name) = var_name {
                    variadic_param = Some(FunctionParam {
                        name,
                        type_expr,
                        span: block.span.clone(),
                    });
                }
            }
        }

        let span = Span::new(
            ident.span.start_byte,
            close_brace_span.end_byte,
            ident.span.start_line,
            ident.span.start_col,
            close_brace_span.end_line,
            close_brace_span.end_col,
        );

        Some(
            FunctionBlock::new(name, params, return_type, result_expr, span)
                .with_variadic_param(variadic_param),
        )
    }

    fn finish_parsing_dynamic_block(&mut self, ident: &Token) -> Option<DynamicBlock> {
        let (block_type, type_span) = match self.advance() {
            Some(t) if t.kind == TokenKind::String => {
                let name = t.text.trim_matches('"').to_string();
                (name, t.span)
            }
            Some(t) if t.kind == TokenKind::Ident => (t.text.clone(), t.span),
            Some(bad) => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error(
                    "Invalid dynamic block definition".to_string(),
                    "A dynamic block definition requires a label specifying the block type to generate.".to_string(),
                    bad.span,
                ));
                self.recover_after_body_item();
                return None;
            }
            None => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error(
                    "Unexpected EOF".to_string(),
                    "Expected dynamic block type label.".to_string(),
                    ident.span.clone(),
                ));
                return None;
            }
        };

        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }

        let open_brace = match self.advance() {
            Some(t) if t.kind == TokenKind::OBrace => t,
            Some(bad) => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error(
                    "Block opening brace expected".to_string(),
                    "Expected '{' after dynamic block type label.".to_string(),
                    bad.span,
                ));
                self.recover_after_body_item();
                return None;
            }
            None => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error(
                    "Unexpected EOF".to_string(),
                    "Expected '{' to start dynamic block body.".to_string(),
                    type_span.clone(),
                ));
                return None;
            }
        };

        let inner_body = self.parse_body_inner(Some(&TokenKind::CBrace));

        let close_brace_span = if let Some(t) = self.advance() {
            t.span
        } else {
            #[rustfmt::skip]
            self.diags.push(Diagnostic::error(
                "Unclosed block".to_string(),
                "Expected '}' to close dynamic block body.".to_string(),
                open_brace.span.clone(),
            ));
            open_brace.span
        };

        let span = Span::new(
            ident.span.start_byte,
            close_brace_span.end_byte,
            ident.span.start_line,
            ident.span.start_col,
            close_brace_span.end_line,
            close_brace_span.end_col,
        );

        let Some(for_each_attr) = inner_body.attributes.get("for_each") else {
            #[rustfmt::skip]
            self.diags.push(Diagnostic::error(
                "Missing required argument".to_string(),
                "A dynamic block must contain a 'for_each' argument.".to_string(),
                span,
            ));
            return None;
        };
        let for_each = for_each_attr.expr.clone();

        let iterator = inner_body.attributes.get("iterator").and_then(|attr| {
            if let Expression::Variable(name, _) = &attr.expr {
                Some(name.clone())
            } else {
                self.diags.push(Diagnostic::error(
                    "Invalid iterator argument".to_string(),
                    "The 'iterator' argument must be an identifier.".to_string(),
                    attr.expr.span(),
                ));
                None
            }
        });

        let labels = inner_body.attributes.get("labels").and_then(|attr| {
            if let Expression::Tuple(elems, _) = &attr.expr {
                Some(elems.clone())
            } else {
                self.diags.push(Diagnostic::error(
                    "Invalid labels argument".to_string(),
                    "The 'labels' argument must be a tuple of expressions.".to_string(),
                    attr.expr.span(),
                ));
                None
            }
        });

        let Some(content_block) = inner_body.blocks.iter().find(|b| b.block_type == "content")
        else {
            self.diags.push(Diagnostic::error(
                "Missing required block",
                "A dynamic block must contain a 'content' block.",
                span,
            ));
            return None;
        };

        let content_body = content_block.body.clone();

        Some(DynamicBlock::new(
            block_type,
            for_each,
            iterator,
            labels,
            content_body,
            span,
            type_span,
        ))
    }

    fn finish_parsing_assertion_block(
        &mut self,
        ident: &Token,
        block_type: &str,
    ) -> Option<(Expression, Expression, Span)> {
        let open_brace = self.advance()?;

        let inner_body = self.parse_body_inner(Some(&TokenKind::CBrace));

        let close_brace_span = if let Some(t) = self.advance() {
            t.span
        } else {
            #[rustfmt::skip]
            self.diags.push(Diagnostic::error(
                "Unclosed block".to_string(),
                format!("Expected '}}' to close {block_type} block body."),
                open_brace.span.clone(),
            ));
            open_brace.span
        };

        let span = Span::new(
            ident.span.start_byte,
            close_brace_span.end_byte,
            ident.span.start_line,
            ident.span.start_col,
            close_brace_span.end_line,
            close_brace_span.end_col,
        );

        let Some(condition_attr) = inner_body.attributes.get("condition") else {
            #[rustfmt::skip]
            self.diags.push(Diagnostic::error(
                "Missing required argument".to_string(),
                format!("A {block_type} block must contain a 'condition' argument."),
                span,
            ));
            return None;
        };

        let Some(error_msg_attr) = inner_body.attributes.get("error_message") else {
            #[rustfmt::skip]
            self.diags.push(Diagnostic::error(
                "Missing required argument".to_string(),
                format!("A {block_type} block must contain an 'error_message' argument."),
                span,
            ));
            return None;
        };

        Some((
            condition_attr.expr.clone(),
            error_msg_attr.expr.clone(),
            span,
        ))
    }

    fn finish_parsing_validation(&mut self, ident: &Token) -> Option<ValidationBlock> {
        let (condition, error_message, span) =
            self.finish_parsing_assertion_block(ident, "validation")?;
        Some(ValidationBlock::new(condition, error_message, span))
    }

    fn finish_parsing_precondition(&mut self, ident: &Token) -> Option<PreconditionBlock> {
        let (condition, error_message, span) =
            self.finish_parsing_assertion_block(ident, "precondition")?;
        Some(PreconditionBlock::new(condition, error_message, span))
    }

    fn finish_parsing_postcondition(&mut self, ident: &Token) -> Option<PostconditionBlock> {
        let (condition, error_message, span) =
            self.finish_parsing_assertion_block(ident, "postcondition")?;
        Some(PostconditionBlock::new(condition, error_message, span))
    }

    fn finish_parsing_block(
        &mut self,
        ident: Token,
        leading_comments: Vec<String>,
    ) -> Option<Block> {
        let mut labels = Vec::new();
        let mut label_spans = Vec::new();

        let open_brace = loop {
            match self.peek_kind() {
                Some(TokenKind::OBrace) => {
                    break self.advance()?;
                }
                Some(TokenKind::String) => {
                    let tok = self.advance()?;
                    let raw = tok.text.strip_prefix('"').unwrap_or(&tok.text);
                    let raw = raw.strip_suffix('"').unwrap_or(raw);
                    match crate::lex::unescape_string(raw, &tok.span) {
                        Ok(unquoted) => {
                            labels.push(unquoted);
                            label_spans.push(tok.span);
                        }
                        Err(err) => {
                            self.diags.push(Diagnostic::new(err, tok.span.clone()));
                            labels.push(raw.to_string());
                            label_spans.push(tok.span);
                        }
                    }
                }
                Some(TokenKind::Ident) => {
                    let tok = self.advance()?;
                    labels.push(tok.text);
                    label_spans.push(tok.span);
                }
                Some(TokenKind::Assign) => {
                    let bad = self.advance()?;
                    #[rustfmt::skip]
                    self.diags.push(Diagnostic::error( "Invalid block definition".to_string(), "The equals sign \"=\" indicates an argument definition, and must not be used when defining a block.".to_string(), bad.span, ));
                    self.recover_after_body_item();
                    return None;
                }
                Some(TokenKind::Newline) => {
                    let bad = self.advance()?;
                    #[rustfmt::skip]
                    self.diags.push(Diagnostic::error( "Invalid block definition".to_string(), "A block definition must have block content delimited by \"{\" and \"}\", starting on the same line as the block header.".to_string(), bad.span, ));
                    self.recover_after_body_item();
                    return None;
                }
                Some(_) | None => {
                    let span = match &self.current {
                        Some(t) => t.span.clone(),
                        None => ident.span.clone(),
                    };
                    #[rustfmt::skip]
                    self.diags.push(Diagnostic::error( "Invalid block definition".to_string(), "Either a quoted string block label or an opening brace (\"{\") is expected here.".to_string(), span, ));
                    self.recover_after_body_item();
                    return None;
                }
            }
        };

        let body = self.parse_body_inner(Some(&TokenKind::CBrace));

        // Expect CBrace
        let close_brace_span = if let Some(cbrace) = self.advance() {
            cbrace.span
        } else {
            let span = open_brace.span.clone();
            #[rustfmt::skip]
            self.diags.push(Diagnostic::error( "Unclosed configuration block".to_string(), "There is no closing brace for this block before the end of the file.".to_string(), span.clone(), ));
            span
        };

        let trailing_comment = self.trailing_comment.take();

        // Must end with a newline or EOF.
        let next = self.peek_kind();
        match next {
            Some(TokenKind::Newline) | None => {
                if next.is_some() {
                    self.advance(); // eat newline
                }
            }
            _ => {
                let bad = self.current.as_ref()?;
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Missing newline after block definition".to_string(), "A block definition must end with a newline.".to_string(), bad.span.clone(), ));
                self.recover_after_body_item();
            }
        }

        Some(Block {
            block_type: ident.text,
            labels,
            body,
            span: Span::new(
                ident.span.start_byte,
                close_brace_span.end_byte,
                ident.span.start_line,
                ident.span.start_col,
                close_brace_span.end_line,
                close_brace_span.end_col,
            ),
            type_span: ident.span,
            label_spans,
            open_brace_span: open_brace.span,
            close_brace_span,
            leading_comments,
            trailing_comment,
        })
    }

    /// Recovers from an error by skipping until a newline or EOF,
    /// attempting to resync at the start of the next body item.
    fn recover_after_body_item(&mut self) {
        loop {
            match self.peek_kind() {
                Some(TokenKind::Newline) => {
                    self.advance();
                    break;
                }
                Some(TokenKind::OBrace) => {
                    // Try to skip the whole block to recover
                    self.advance();
                    let mut depth = 1;
                    loop {
                        match self.peek_kind() {
                            Some(TokenKind::OBrace) => {
                                depth += 1;
                                self.advance();
                            }
                            Some(TokenKind::CBrace) => {
                                depth -= 1;
                                self.advance();
                                if depth == 0 {
                                    break;
                                }
                            }
                            None => break,
                            _ => {
                                self.advance();
                            }
                        }
                    }
                    break; // stop recovering after we cleared the block
                }
                None => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Parses an expression.
    pub fn parse_expression(&mut self) -> Option<Expression> {
        self.parse_expression_precedence(crate::parse::Precedence::Lowest)
    }

    fn parse_expression_precedence(
        &mut self,
        precedence: crate::parse::Precedence,
    ) -> Option<Expression> {
        let prefix_tok = self.advance()?;
        let mut left = self.parse_prefix(prefix_tok)?;

        while let Some(next_kind) = self.peek_kind() {
            let next_prec = crate::parse::Precedence::from_token(&next_kind);
            if precedence >= next_prec {
                break;
            }

            let infix_tok = self.advance()?;
            left = self.parse_infix(left, &infix_tok)?;
        }

        Some(left)
    }

    /// Parses the content of a string or heredoc into template parts.
    pub(crate) fn parse_template(
        diags: &mut Diagnostics,
        text: &str,
        span: &Span,
    ) -> Vec<crate::ast::expr::TemplatePart> {
        use crate::ast::expr::{Directive, TemplatePart};

        #[derive(Clone, Debug)]
        enum RawDirective {
            If(Expression),
            ElseIf(Expression),
            Else,
            EndIf,
            For(Option<String>, String, Expression),
            EndFor,
        }

        #[derive(Clone, Debug)]
        enum RawItem {
            Literal(String, Span),
            Interpolation(Expression, Span, bool, bool),
            Directive(RawDirective, Span, bool, bool),
        }

        let mut raw_items = Vec::new();
        let is_heredoc = text.starts_with("<<");
        let is_indented_heredoc = text.starts_with("<<-");
        let content_start = if is_heredoc {
            text.find('\n').map_or(text.len(), |i| i + 1)
        } else {
            usize::from(text.starts_with('"'))
        };
        let content_end = if is_heredoc {
            text.rfind('\n').unwrap_or(text.len())
        } else if text.ends_with('"') {
            text.len().saturating_sub(1)
        } else {
            text.len()
        };
        let raw_content = if content_start <= content_end {
            &text[content_start..content_end]
        } else {
            ""
        };

        let normalized_content = if is_indented_heredoc {
            match strip_heredoc_indentation(raw_content) {
                Ok(stripped) => stripped,
                Err(err) => {
                    diags.push(crate::diagnostic::Diagnostic::new(err, span.clone()));
                    raw_content.to_string()
                }
            }
        } else {
            raw_content.to_string()
        };
        let content: &str = &normalized_content;
        let mut i = 0;
        while i < content.len() {
            if content[i..].starts_with("${") {
                let start = i;
                let is_strip_left = content[i..].starts_with("${~");
                if is_strip_left {
                    i += 3;
                } else {
                    i += 2;
                }
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
                let expr_str_start = start + if is_strip_left { 3 } else { 2 };
                let expr_str_end = if is_strip_right { end - 2 } else { end - 1 };
                let expr_str_raw = &content[expr_str_start..expr_str_end];
                let expr_str = if is_heredoc {
                    expr_str_raw.to_string()
                } else {
                    expr_str_raw.replace("\\\"", "\"")
                };
                let mut sub_parser = Parser::new(&expr_str);
                if let Some(expr) = sub_parser.parse_expression() {
                    raw_items.push(RawItem::Interpolation(
                        expr,
                        span.clone(),
                        is_strip_left,
                        is_strip_right,
                    ));
                }
                i = end;
            } else if content[i..].starts_with("%{") {
                let start = i;
                let is_strip_left = content[i..].starts_with("%{~");
                if is_strip_left {
                    i += 3;
                } else {
                    i += 2;
                }
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
                let directive_str_start = start + if is_strip_left { 3 } else { 2 };
                let directive_str_end = if is_strip_right { end - 2 } else { end - 1 };
                let directive_str_raw = content[directive_str_start..directive_str_end].trim();
                let directive_str = if is_heredoc {
                    directive_str_raw.to_string()
                } else {
                    directive_str_raw.replace("\\\"", "\"")
                };

                let directive = if directive_str == "endif" {
                    Some(RawDirective::EndIf)
                } else if directive_str == "endfor" {
                    Some(RawDirective::EndFor)
                } else if directive_str == "else" {
                    Some(RawDirective::Else)
                } else if let Some(stripped) = directive_str.strip_prefix("if ") {
                    let mut cond_parser = Parser::new(stripped.trim());
                    cond_parser.parse_expression().map(RawDirective::If)
                } else if let Some(stripped) = directive_str
                    .strip_prefix("else if ")
                    .or_else(|| directive_str.strip_prefix("elif "))
                {
                    let mut cond_parser = Parser::new(stripped.trim());
                    cond_parser.parse_expression().map(RawDirective::ElseIf)
                } else if let Some(stripped) = directive_str.strip_prefix("for ") {
                    let body_str = stripped.trim();
                    if let Some(in_pos) = body_str.find(" in ") {
                        let var_part = body_str[..in_pos].trim();
                        let coll_part = body_str[in_pos + 4..].trim();
                        let (key_var, val_var) = if let Some(comma_pos) = var_part.find(',') {
                            (
                                Some(var_part[..comma_pos].trim().to_string()),
                                var_part[comma_pos + 1..].trim().to_string(),
                            )
                        } else {
                            (None, var_part.to_string())
                        };
                        let mut coll_parser = Parser::new(coll_part);
                        coll_parser
                            .parse_expression()
                            .map(|collection| RawDirective::For(key_var, val_var, collection))
                    } else {
                        diags.push(crate::diagnostic::Diagnostic::error(
                            "Invalid %{ for } directive".to_string(),
                            "Expected `in` keyword in for directive, e.g., `%{ for val in collection }`"
                                .to_string(),
                            span.clone(),
                        ));
                        None
                    }
                } else {
                    diags.push(crate::diagnostic::Diagnostic::error(
                        "Unknown template directive".to_string(),
                        format!("Unknown directive `%{directive_str}`"),
                        span.clone(),
                    ));
                    None
                };

                if let Some(d) = directive {
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
                    let mut matched = false;
                    if content[i..].starts_with("$$") && content[i + 2..].starts_with('{') {
                        literal_content.push_str("${");
                        i += 3;
                        matched = true;
                    } else if content[i..].starts_with("%%") && content[i + 2..].starts_with('{') {
                        literal_content.push_str("%{");
                        i += 3;
                        matched = true;
                    }
                    if matched {
                        continue;
                    }
                    if content[i..].starts_with("${") || content[i..].starts_with("%{") {
                        break;
                    }
                    if !is_heredoc && content[i..].starts_with('\\') {
                        i += 1;
                        if let Some(c) = content[i..].chars().next() {
                            match c {
                                'n' => {
                                    literal_content.push('\n');
                                    i += 1;
                                }
                                'r' => {
                                    literal_content.push('\r');
                                    i += 1;
                                }
                                't' => {
                                    literal_content.push('\t');
                                    i += 1;
                                }
                                '\\' => {
                                    literal_content.push('\\');
                                    i += 1;
                                }
                                '"' => {
                                    literal_content.push('"');
                                    i += 1;
                                }
                                'u' => {
                                    i += 1;
                                    let code =
                                        u32::from_str_radix(&content[i..i + 4], 16).unwrap_or(0);
                                    if let Some(ch) = char::from_u32(code) {
                                        literal_content.push(ch);
                                    } else {
                                        #[rustfmt::skip]
                                        diags.push(crate::diagnostic::Diagnostic::error("Invalid Unicode escape".to_string(), "Invalid Unicode scalar value.".to_string(), span.clone()));
                                    }
                                    i += 4;
                                }
                                'U' => {
                                    i += 1;
                                    let code =
                                        u32::from_str_radix(&content[i..i + 8], 16).unwrap_or(0);
                                    if let Some(ch) = char::from_u32(code) {
                                        literal_content.push(ch);
                                    } else {
                                        #[rustfmt::skip]
                                        diags.push(crate::diagnostic::Diagnostic::error("Invalid Unicode escape".to_string(), "Invalid Unicode scalar value.".to_string(), span.clone()));
                                    }
                                    i += 8;
                                }
                                _ => {
                                    literal_content.push(c);
                                    i += c.len_utf8();
                                }
                            }
                        } else {
                            literal_content.push('\\');
                            i += 1;
                        }
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

        for idx in 0..raw_items.len() {
            let (strip_left, strip_right) = match &raw_items[idx] {
                RawItem::Literal(_, _) => (false, false),
                RawItem::Interpolation(_, _, sl, sr) | RawItem::Directive(_, _, sl, sr) => {
                    (*sl, *sr)
                }
            };
            if strip_left
                && idx > 0
                && let RawItem::Literal(ref mut s, _) = raw_items[idx - 1]
            {
                *s = s.trim_end().to_string();
            }
            if strip_right
                && idx + 1 < raw_items.len()
                && let RawItem::Literal(ref mut s, _) = raw_items[idx + 1]
            {
                *s = s.trim_start().to_string();
            }
        }

        fn is_end_if(item: Option<&RawItem>) -> bool {
            matches!(item, Some(RawItem::Directive(RawDirective::EndIf, ..)))
        }

        fn is_end_for(item: Option<&RawItem>) -> bool {
            matches!(item, Some(RawItem::Directive(RawDirective::EndFor, ..)))
        }

        fn build_template_tree(
            items: &[RawItem],
            pos: &mut usize,
            diags: &mut Diagnostics,
            terminators: &[&str],
        ) -> Vec<TemplatePart> {
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
                            let if_span = sp.clone();
                            let cond = cond.clone();
                            *pos += 1;
                            let true_expr = build_template_tree(
                                items,
                                pos,
                                diags,
                                &["elseif", "else", "endif"],
                            );
                            let mut else_ifs = Vec::new();
                            while *pos < items.len() {
                                if let RawItem::Directive(RawDirective::ElseIf(elif_cond), ..) =
                                    &items[*pos]
                                {
                                    let elif_cond = elif_cond.clone();
                                    *pos += 1;
                                    let elif_body = build_template_tree(
                                        items,
                                        pos,
                                        diags,
                                        &["elseif", "else", "endif"],
                                    );
                                    else_ifs.push((elif_cond, elif_body));
                                } else {
                                    break;
                                }
                            }
                            let mut false_expr = None;
                            if *pos < items.len()
                                && let RawItem::Directive(RawDirective::Else, ..) = &items[*pos]
                            {
                                *pos += 1;
                                false_expr =
                                    Some(build_template_tree(items, pos, diags, &["endif"]));
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
                            parts.push(TemplatePart::Directive(
                                Directive::If {
                                    cond,
                                    true_expr,
                                    else_ifs,
                                    false_expr,
                                },
                                if_span,
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
                        RawDirective::ElseIf(_) if terminators.contains(&"elseif") => {
                            return parts;
                        }
                        RawDirective::Else if terminators.contains(&"else") => {
                            return parts;
                        }
                        RawDirective::EndIf if terminators.contains(&"endif") => {
                            return parts;
                        }
                        RawDirective::EndFor if terminators.contains(&"endfor") => {
                            return parts;
                        }
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

        let mut pos = 0;
        build_template_tree(&raw_items, &mut pos, diags, &[])
    }

    fn parse_prefix(&mut self, tok: Token) -> Option<Expression> {
        match tok.kind {
            TokenKind::Number => {
                let n = tok
                    .text
                    .parse()
                    .unwrap_or_else(|_| crate::number::Number(bigdecimal::BigDecimal::from(0)));
                Some(Expression::Number(n, tok.span))
            }

            TokenKind::String | TokenKind::Heredoc => {
                let template_parts = Self::parse_template(&mut self.diags, &tok.text, &tok.span);
                if template_parts.len() == 1
                    && let crate::ast::expr::TemplatePart::Literal(ref s, _) = template_parts[0]
                    && tok.kind == TokenKind::String
                {
                    return Some(Expression::String(s.clone(), tok.span));
                }
                Some(Expression::Template(template_parts, tok.span))
            }

            TokenKind::ColonColon => {
                self.diags.push(Diagnostic::error(
                    "Syntax error",
                    "Unexpected leading '::'",
                    tok.span,
                ));
                None
            }

            TokenKind::Ident if tok.text == "null" => Some(Expression::Null(tok.span)),
            TokenKind::Ident if tok.text == "true" => Some(Expression::Bool(true, tok.span)),
            TokenKind::Ident if tok.text == "false" => Some(Expression::Bool(false, tok.span)),
            TokenKind::Ident => {
                let mut name = tok.text;
                let mut span = tok.span;
                let mut has_ns = false;
                while self.peek_kind() == Some(TokenKind::ColonColon) {
                    has_ns = true;
                    let cc_tok = self.advance()?;
                    span = span.merge(&cc_tok.span);
                    if self.peek_kind() == Some(TokenKind::Ident) {
                        let next_ident = self.advance()?;
                        name.push_str("::");
                        name.push_str(&next_ident.text);
                        span = span.merge(&next_ident.span);
                    } else {
                        self.diags.push(Diagnostic::error(
                            "Syntax error",
                            "Expected identifier after '::'",
                            cc_tok.span,
                        ));
                        return None;
                    }
                }
                if has_ns && self.peek_kind() != Some(TokenKind::OParen) {
                    self.diags.push(Diagnostic::error(
                        "Syntax error",
                        "Expected function call after namespaced identifier",
                        span.clone(),
                    ));
                }
                Some(Expression::Variable(name, span))
            }
            TokenKind::OParen => {
                let inner = self.parse_expression()?;
                let cparen = self.advance()?;
                if cparen.kind != TokenKind::CParen {
                    #[rustfmt::skip]
                    self.diags.push(Diagnostic::error( "Expected closing parenthesis".to_string(), "Expected `)` to close the expression.".to_string(), cparen.span, ));
                    return None;
                }
                let span = Span::new(
                    tok.span.start_byte,
                    cparen.span.end_byte,
                    tok.span.start_line,
                    tok.span.start_col,
                    cparen.span.end_line,
                    cparen.span.end_col,
                );
                Some(Expression::Parentheses(Box::new(inner), span))
            }
            TokenKind::OBrack => {
                let mut elements = Vec::new();
                let end_span = loop {
                    if self.peek_kind() == Some(TokenKind::CBrack) {
                        break self.advance()?.span;
                    }
                    if self.peek_kind() == Some(TokenKind::Newline) {
                        self.advance();
                        continue;
                    }
                    {
                        let expr = self.parse_expression()?;
                        elements.push(expr);
                    }
                    match self.peek_kind() {
                        Some(TokenKind::Comma | TokenKind::Newline) => {
                            self.advance();
                        }
                        Some(TokenKind::CBrack) => {
                            break self.advance()?.span;
                        }
                        _ => {
                            let bad = self.advance()?;
                            #[rustfmt::skip]
                            self.diags.push(Diagnostic::error( "Expected comma or `]`".to_string(), "Expected a comma to separate tuple elements, or `]` to close the tuple.".to_string(), bad.span, ));
                            return None;
                        }
                    }
                };
                let span = Span::new(
                    tok.span.start_byte,
                    end_span.end_byte,
                    tok.span.start_line,
                    tok.span.start_col,
                    end_span.end_line,
                    end_span.end_col,
                );
                Some(Expression::Tuple(elements, span))
            }
            TokenKind::OBrace => {
                let mut elements = Vec::new();
                let end_span = loop {
                    if self.peek_kind() == Some(TokenKind::CBrace) {
                        break self.advance()?.span;
                    }
                    if self.peek_kind() == Some(TokenKind::Newline) {
                        self.advance();
                        continue;
                    }
                    let key = self.parse_expression()?;

                    let op = self.advance()?;
                    if op.kind != TokenKind::Assign && op.kind != TokenKind::Colon {
                        #[rustfmt::skip]
                        self.diags.push(Diagnostic::error( "Expected `=` or `:`".to_string(), "Expected `=` or `:` in object constructor.".to_string(), op.span, ));
                        return None;
                    }

                    let val = self.parse_expression()?;

                    elements.push((key, val));

                    match self.peek_kind() {
                        Some(TokenKind::Comma | TokenKind::Newline) => {
                            self.advance();
                        }
                        Some(TokenKind::CBrace) => {
                            break self.advance()?.span;
                        }
                        _ => {
                            let bad = self.advance()?;
                            #[rustfmt::skip]
                            self.diags.push(Diagnostic::error( "Expected comma, newline, or `}`".to_string(), "Expected a separator or `}` in object constructor.".to_string(), bad.span, ));
                            return None;
                        }
                    }
                };
                let span = Span::new(
                    tok.span.start_byte,
                    end_span.end_byte,
                    tok.span.start_line,
                    tok.span.start_col,
                    end_span.end_line,
                    end_span.end_col,
                );
                Some(Expression::Object(elements, span))
            }
            TokenKind::Minus => {
                let right = self.parse_expression_precedence(crate::parse::Precedence::Unary)?;
                let span = Span::new(
                    tok.span.start_byte,
                    right.span().end_byte,
                    tok.span.start_line,
                    tok.span.start_col,
                    right.span().end_line,
                    right.span().end_col,
                );
                Some(Expression::UnaryOp(
                    crate::ast::expr::UnaryOp::Neg,
                    Box::new(right),
                    span,
                ))
            }
            TokenKind::Not => {
                let right = self.parse_expression_precedence(crate::parse::Precedence::Unary)?;
                let span = Span::new(
                    tok.span.start_byte,
                    right.span().end_byte,
                    tok.span.start_line,
                    tok.span.start_col,
                    right.span().end_line,
                    right.span().end_col,
                );
                Some(Expression::UnaryOp(
                    crate::ast::expr::UnaryOp::Not,
                    Box::new(right),
                    span,
                ))
            }
            _ => {
                #[rustfmt::skip]
                self.diags.push(Diagnostic::error( "Expected expression".to_string(), "An expression was expected here.".to_string(), tok.span, ));
                None
            }
        }
    }

    fn parse_infix(&mut self, left: Expression, tok: &Token) -> Option<Expression> {
        match tok.kind {
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::Eq
            | TokenKind::NotEq
            | TokenKind::Lt
            | TokenKind::Lte
            | TokenKind::Gt
            | TokenKind::Gte
            | TokenKind::And
            | TokenKind::Or => {
                let prec = crate::parse::Precedence::from_token(&tok.kind);
                let right = self.parse_expression_precedence(prec)?;
                let op = match tok.kind {
                    TokenKind::Plus => crate::ast::expr::BinaryOp::Add,
                    TokenKind::Minus => crate::ast::expr::BinaryOp::Sub,
                    TokenKind::Star => crate::ast::expr::BinaryOp::Mul,
                    TokenKind::Slash => crate::ast::expr::BinaryOp::Div,
                    TokenKind::Percent => crate::ast::expr::BinaryOp::Mod,
                    TokenKind::Eq => crate::ast::expr::BinaryOp::Eq,
                    TokenKind::NotEq => crate::ast::expr::BinaryOp::NotEq,
                    TokenKind::Lt => crate::ast::expr::BinaryOp::Less,
                    TokenKind::Lte => crate::ast::expr::BinaryOp::LessEq,
                    TokenKind::Gt => crate::ast::expr::BinaryOp::Greater,
                    TokenKind::Gte => crate::ast::expr::BinaryOp::GreaterEq,
                    TokenKind::And => crate::ast::expr::BinaryOp::And,
                    _ => crate::ast::expr::BinaryOp::Or,
                };
                let span = Span::new(
                    left.span().start_byte,
                    right.span().end_byte,
                    left.span().start_line,
                    left.span().start_col,
                    right.span().end_line,
                    right.span().end_col,
                );
                Some(Expression::BinaryOp(
                    op,
                    Box::new(left),
                    Box::new(right),
                    span,
                ))
            }
            TokenKind::Question => {
                let true_expr = self.parse_expression()?;
                let colon = self.advance()?;
                if colon.kind != TokenKind::Colon {
                    #[rustfmt::skip]
                    self.diags.push(Diagnostic::error( "Expected `:`".to_string(), "Expected `:` for the conditional expression.".to_string(), colon.span, ));
                    return None;
                }
                let false_expr =
                    self.parse_expression_precedence(crate::parse::Precedence::Conditional)?;
                let span = Span::new(
                    left.span().start_byte,
                    false_expr.span().end_byte,
                    left.span().start_line,
                    left.span().start_col,
                    false_expr.span().end_line,
                    false_expr.span().end_col,
                );
                let conditional = crate::ast::expr::Conditional {
                    cond_expr: left,
                    true_expr,
                    false_expr,
                };
                Some(Expression::Conditional(Box::new(conditional), span))
            }
            TokenKind::Dot => {
                let attr_tok = self.advance()?;
                let op = match attr_tok.kind {
                    TokenKind::Ident => crate::ast::expr::TraversalOperator::GetAttr(
                        attr_tok.text,
                        attr_tok.span.clone(),
                    ),
                    TokenKind::Star => {
                        let op_span = Span::new(
                            tok.span.start_byte,
                            attr_tok.span.end_byte,
                            tok.span.start_line,
                            tok.span.start_col,
                            attr_tok.span.end_line,
                            attr_tok.span.end_col,
                        );
                        crate::ast::expr::TraversalOperator::AttrSplat(op_span)
                    }
                    TokenKind::Number => {
                        if let Ok(idx) = attr_tok.text.parse::<u64>() {
                            let op_span = Span::new(
                                tok.span.start_byte,
                                attr_tok.span.end_byte,
                                tok.span.start_line,
                                tok.span.start_col,
                                attr_tok.span.end_line,
                                attr_tok.span.end_col,
                            );
                            crate::ast::expr::TraversalOperator::LegacyIndex(idx, op_span)
                        } else {
                            #[rustfmt::skip]
                            self.diags.push(Diagnostic::error( "Expected integer index".to_string(), "Expected an integer for legacy index after `.`.".to_string(), attr_tok.span, ));
                            return None;
                        }
                    }
                    _ => {
                        #[rustfmt::skip]
                        self.diags.push(Diagnostic::error( "Expected identifier".to_string(), "Expected attribute name after `.`.".to_string(), attr_tok.span, ));
                        return None;
                    }
                };
                let span = Span::new(
                    left.span().start_byte,
                    attr_tok.span.end_byte,
                    left.span().start_line,
                    left.span().start_col,
                    attr_tok.span.end_line,
                    attr_tok.span.end_col,
                );
                let mut operators = Vec::new();
                let base_expr = if let Expression::Traversal(t, _) = left {
                    operators = t.operators;
                    t.expr
                } else {
                    Box::new(left)
                };
                operators.push(op);
                let traversal = crate::ast::expr::Traversal {
                    expr: base_expr,
                    operators,
                };
                Some(Expression::Traversal(Box::new(traversal), span))
            }
            TokenKind::OBrack => {
                let (op, cbrack_span) = if self.peek_kind() == Some(TokenKind::Star) {
                    let _star_tok = self.advance()?;
                    let Some(cbrack) = self.advance() else {
                        #[rustfmt::skip]
                        self.diags.push(Diagnostic::error(
                            "Expected `]`".to_string(),
                            "Expected `]` to close the full splat operator.".to_string(),
                            tok.span.clone(),
                        ));
                        return None;
                    };
                    if cbrack.kind != TokenKind::CBrack {
                        #[rustfmt::skip]
                        self.diags.push(Diagnostic::error( "Expected `]`".to_string(), "Expected `]` to close the full splat operator.".to_string(), cbrack.span, ));
                        return None;
                    }
                    let op_span = Span::new(
                        tok.span.start_byte,
                        cbrack.span.end_byte,
                        tok.span.start_line,
                        tok.span.start_col,
                        cbrack.span.end_line,
                        cbrack.span.end_col,
                    );
                    (
                        crate::ast::expr::TraversalOperator::FullSplat(op_span),
                        cbrack.span,
                    )
                } else {
                    let index_expr = self.parse_expression()?;
                    let Some(cbrack) = self.advance() else {
                        #[rustfmt::skip]
                        self.diags.push(Diagnostic::error(
                            "Expected `]`".to_string(),
                            "Expected `]` to close the index expression.".to_string(),
                            tok.span.clone(),
                        ));
                        return None;
                    };
                    if cbrack.kind != TokenKind::CBrack {
                        #[rustfmt::skip]
                        self.diags.push(Diagnostic::error( "Expected `]`".to_string(), "Expected `]` to close the index expression.".to_string(), cbrack.span, ));
                        return None;
                    }
                    let op_span = Span::new(
                        tok.span.start_byte,
                        cbrack.span.end_byte,
                        tok.span.start_line,
                        tok.span.start_col,
                        cbrack.span.end_line,
                        cbrack.span.end_col,
                    );
                    (
                        crate::ast::expr::TraversalOperator::Index(index_expr, op_span),
                        cbrack.span,
                    )
                };
                let span = Span::new(
                    left.span().start_byte,
                    cbrack_span.end_byte,
                    left.span().start_line,
                    left.span().start_col,
                    cbrack_span.end_line,
                    cbrack_span.end_col,
                );
                let mut operators = Vec::new();
                let base_expr = if let Expression::Traversal(t, _) = left {
                    operators = t.operators;
                    t.expr
                } else {
                    Box::new(left)
                };
                operators.push(op);
                let traversal = crate::ast::expr::Traversal {
                    expr: base_expr,
                    operators,
                };
                Some(Expression::Traversal(Box::new(traversal), span))
            }
            _ => {
                let mut args = Vec::new();
                let (cparen_tok, expand_final) = if self.peek_kind() == Some(TokenKind::CParen) {
                    (self.advance()?, false)
                } else {
                    let mut expand = false;
                    let cparen = loop {
                        {
                            let expr = self.parse_expression()?;
                            args.push(expr);
                        }

                        match self.peek_kind() {
                            Some(TokenKind::Comma) => {
                                self.advance();
                            }
                            Some(TokenKind::Ellipsis) => {
                                self.advance();
                                expand = true;
                                let next = self.advance()?;
                                if next.kind != TokenKind::CParen {
                                    #[rustfmt::skip]
                                    self.diags.push(Diagnostic::error( "Expected `)`".to_string(), "Expected `)` after `...`.".to_string(), next.span, ));
                                    return None;
                                }
                                break next;
                            }
                            Some(TokenKind::CParen) => {
                                break self.advance()?;
                            }
                            _ => {
                                let bad = self.advance()?;
                                #[rustfmt::skip]
                                self.diags.push(Diagnostic::error( "Expected `,` or `)`".to_string(), "Expected a comma to separate function arguments, or `)` to close the call.".to_string(), bad.span, ));
                                return None;
                            }
                        }
                    };
                    (cparen, expand)
                };

                let span = Span::new(
                    left.span().start_byte,
                    cparen_tok.span.end_byte,
                    left.span().start_line,
                    left.span().start_col,
                    cparen_tok.span.end_line,
                    cparen_tok.span.end_col,
                );

                let name = if let Expression::Variable(ref name, _) = left {
                    crate::ast::expr::NamespacedIdent::parse(name, left.span())
                } else {
                    #[rustfmt::skip]
                    self.diags.push(Diagnostic::error( "Invalid function call".to_string(), "Only top-level identifiers can be called as functions.".to_string(), left.span(), ));
                    crate::ast::expr::NamespacedIdent::simple("invalid_call", left.span())
                };

                let func_call = crate::ast::expr::FuncCall {
                    name,
                    args,
                    expand_final,
                };
                Some(Expression::FuncCall(Box::new(func_call), span))
            }
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
