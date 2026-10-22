//! Recursive-descent parser for HCL native syntax.
use crate::ast::expr::Expression;
use crate::ast::structure::{
    Attribute, Block, Body, DynamicBlock, PostconditionBlock, PreconditionBlock, ValidationBlock,
};
use crate::ast::user_func::{FunctionBlock, FunctionParam};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::lex::lexer::Lexer;
use crate::lex::token::{Token, TokenKind};
pub use crate::parse::template::strip_heredoc_indentation;
use crate::parse::template::{
    apply_template_strip_markers, build_template_tree, tokenize_template_raw_items,
};
use crate::parse::type_expr::TypeExprParser;
use crate::span::Span;
use std::collections::HashMap;
/// The HCL Parser.
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Option<Token>,
    next_tok: Option<Token>,
    diags: Diagnostics,
    leading_comments: Vec<String>,
    trailing_comment: Option<String>,
    last_line: usize,
    paren_depth: usize,
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
            paren_depth: 0,
        }
    }
    /// Return the current diagnostics.
    #[must_use]
    pub const fn errors(&self) -> &Diagnostics {
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
                    diags
                        .push(
                            Diagnostic::error(
                                "Lexical Error".to_string(),
                                format!("Unrecognized token '{}'", err.text),
                                err.span,
                            ),
                        );
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
    /// Advances the parser and returns the consumed token, falling back to an empty token if at EOF.
    fn advance_token(&mut self) -> Token {
        self.advance()
            .unwrap_or_else(|| Token::new(TokenKind::Ident, String::new(), Span::default()))
    }
    /// Returns the span of the current token, or `fallback` if at EOF.
    fn current_span_or(&self, fallback: Span) -> Span {
        self.current.as_ref().map_or(fallback, |t| t.span.clone())
    }
    /// Check the current token's kind without advancing.
    fn peek_kind(&self) -> Option<TokenKind> {
        self.current.as_ref().map(|t| t.kind.clone())
    }
    /// Parse a top-level Body.
    pub fn parse_body(&mut self) -> Body {
        self.parse_body_inner(None)
    }
    fn try_parse_special_body_item(
        &mut self,
        ident_tok: &Token,
        body: &mut Body,
        last_span: &mut Option<Span>,
    ) -> bool {
        if ident_tok.text == "function" {
            if let Some(func_block) = self.finish_parsing_function(ident_tok) {
                *last_span = Some(func_block.span.clone());
                body.functions.push(func_block);
            }
            return true;
        }
        if ident_tok.text == "dynamic" && self.peek_kind() != Some(TokenKind::Assign) {
            if let Some(dyn_block) = self.finish_parsing_dynamic_block(ident_tok) {
                *last_span = Some(dyn_block.span.clone());
                body.dynamic_blocks.push(dyn_block);
            }
            return true;
        }
        if ident_tok.text == "validation" && self.peek_kind() == Some(TokenKind::OBrace) {
            if let Some(val_block) = self.finish_parsing_validation(ident_tok) {
                *last_span = Some(val_block.span.clone());
                body.validations.push(val_block);
            }
            return true;
        }
        if ident_tok.text == "precondition" && self.peek_kind() == Some(TokenKind::OBrace) {
            if let Some(pre_block) = self.finish_parsing_precondition(ident_tok) {
                *last_span = Some(pre_block.span.clone());
                body.preconditions.push(pre_block);
            }
            return true;
        }
        if ident_tok.text == "postcondition" && self.peek_kind() == Some(TokenKind::OBrace) {
            if let Some(post_block) = self.finish_parsing_postcondition(ident_tok) {
                *last_span = Some(post_block.span.clone());
                body.postconditions.push(post_block);
            }
            return true;
        }
        false
    }

    fn parse_body_inner(&mut self, end_token: Option<&TokenKind>) -> Body {
        let mut body = Body {
            attributes: HashMap::new(),
            blocks: Vec::new(),
            functions: Vec::new(),
            dynamic_blocks: Vec::new(),
            validations: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
            span: Span::new(0, 0, 0, 0, 0, 0),
        };
        let mut first_span: Option<Span> = None;
        let mut last_span: Option<Span> = None;
        loop {
            while self.peek_kind() == Some(TokenKind::Newline) {
                self.advance();
            }
            if self.peek_kind().as_ref() == end_token {
                break;
            }
            let Some(ident_tok) = self.advance() else {
                break;
            };
            if ident_tok.kind != TokenKind::Ident {
                self.diags.push(Diagnostic::error(
                    "Argument or block definition required".to_string(),
                    "An argument or block definition is required here.".to_string(),
                    ident_tok.span,
                ));
                self.recover_after_body_item();
                continue;
            }
            if first_span.is_none() {
                first_span = Some(ident_tok.span.clone());
            }
            last_span = Some(ident_tok.span.clone());
            if self.try_parse_special_body_item(&ident_tok, &mut body, &mut last_span) {
                continue;
            }
            let item_leading_comments = std::mem::take(&mut self.leading_comments);
            match self.peek_kind() {
                Some(TokenKind::Assign) => {
                    if let Some(attr) =
                        self.finish_parsing_attribute(ident_tok, item_leading_comments)
                    {
                        last_span = Some(attr.span.clone());
                        if let Some(existing) = body.attributes.get(&attr.name) {
                            self.diags.push(Diagnostic::error(
                                "Attribute redefined".to_string(),
                                format!(
                                    "The argument {:?} was already set at line {}, col {}. Each argument may be set only once.",
                                    attr.name, existing.name_span.start_line, existing.name_span.start_col
                                ),
                                attr.name_span,
                            ));
                        } else {
                            body.attributes.insert(attr.name.clone(), attr);
                        }
                    }
                }
                Some(TokenKind::String | TokenKind::OBrace | TokenKind::Ident) => {
                    if let Some(block) = self.finish_parsing_block(ident_tok, item_leading_comments)
                    {
                        last_span = Some(block.span.clone());
                        body.blocks.push(block);
                    }
                }
                _ => {
                    self.diags.push(Diagnostic::error(
                        "Argument or block definition required".to_string(),
                        "An argument or block definition is required here. To set an argument, use the equals sign \"=\" to introduce the argument value.".to_string(),
                        ident_tok.span,
                    ));
                    self.recover_after_body_item();
                }
            }
        }
        if let (Some(first), Some(last)) = (first_span, last_span) {
            body.span = first.merge(&last);
        }
        body
    }
    fn finish_parsing_attribute(
        &mut self,
        ident: Token,
        leading_comments: Vec<String>,
    ) -> Option<Attribute> {
        let assign_tok = self.advance_token();
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let Some(expr) = self.parse_expression() else {
            if !self.diags.has_errors() {
                let bad_span = self.current_span_or(assign_tok.span);
                #[rustfmt::skip]
                self.diags
                    .push(
                        Diagnostic::error(
                            "Expected expression".to_string(),
                            "An expression was expected after '='.".to_string(),
                            bad_span,
                        ),
                    );
            }
            self.recover_after_body_item();
            return None;
        };
        let end_span = expr.span();
        let trailing_comment = self.trailing_comment.take();
        let next = self.peek_kind();
        match next {
            Some(TokenKind::Newline) | None => {
                if next.is_some() {
                    self.advance();
                }
            }
            Some(TokenKind::CBrace) => {}
            Some(TokenKind::Comma) => {
                let bad = self.advance_token();
                #[rustfmt::skip]
                self.diags
                    .push(
                        Diagnostic::error(
                            "Unexpected comma after argument".to_string(),
                            "Argument definitions must be separated by newlines, not commas. An argument definition must end with a newline."
                                .to_string(),
                            bad.span,
                        ),
                    );
                self.recover_after_body_item();
            }
            Some(_) => {
                let bad_span = self.current_span_or(ident.span.clone());
                #[rustfmt::skip]
                self.diags
                    .push(
                        Diagnostic::error(
                            "Missing newline after argument".to_string(),
                            "An argument definition must end with a newline."
                                .to_string(),
                            bad_span,
                        ),
                    );
                self.recover_after_body_item();
            }
        }
        Some(Attribute {
            name: ident.text,
            expr,
            span: ident.span.merge(&end_span),
            name_span: ident.span,
            equals_span: assign_tok.span,
            leading_comments,
            trailing_comment,
        })
    }
    fn extract_function_block_parts(
        body: &Body,
    ) -> (
        Vec<FunctionParam>,
        Option<crate::ast::type_expr::TypeExpr>,
        Option<FunctionParam>,
    ) {
        let mut params = Vec::new();
        if let Some(p_attr) = body.attributes.get("params") {
            if let Expression::Tuple(elements, _) = &p_attr.expr {
                for el in elements {
                    if let Expression::Variable(name, span) | Expression::String(name, span) = el {
                        params.push(FunctionParam {
                            name: name.clone(),
                            type_expr: None,
                            span: span.clone(),
                        });
                    }
                }
            }
        }
        let return_type = body.attributes.get("return_type").and_then(|rt_attr| {
            if let Expression::Variable(ty_name, _) = &rt_attr.expr {
                TypeExprParser::new(ty_name).parse().ok()
            } else {
                None
            }
        });
        let mut variadic_param =
            body.attributes
                .get("variadic_param")
                .and_then(|vp_attr| match &vp_attr.expr {
                    Expression::Variable(name, span) | Expression::String(name, span) => {
                        Some(FunctionParam {
                            name: name.clone(),
                            type_expr: None,
                            span: span.clone(),
                        })
                    }
                    _ => None,
                });
        for block in &body.blocks {
            if block.block_type == "variadic_param" {
                let var_name = block.body.attributes.get("name").map_or_else(
                    || block.labels.first().cloned(),
                    |n_attr| match &n_attr.expr {
                        Expression::Variable(s, _) | Expression::String(s, _) => Some(s.clone()),
                        _ => None,
                    },
                );
                let type_expr = block.body.attributes.get("type").and_then(|t_attr| {
                    if let Expression::Variable(s, _) | Expression::String(s, _) = &t_attr.expr {
                        TypeExprParser::new(s).parse().ok()
                    } else {
                        None
                    }
                });
                if let Some(name) = var_name {
                    variadic_param = Some(FunctionParam {
                        name,
                        type_expr,
                        span: block.span.clone(),
                    });
                }
            }
        }
        (params, return_type, variadic_param)
    }

    fn finish_parsing_function(&mut self, ident: &Token) -> Option<FunctionBlock> {
        let name_tok = match self.advance() {
            Some(t) if t.kind == TokenKind::String => t,
            Some(bad) => {
                self.diags.push(Diagnostic::error(
                    "Invalid function definition".to_string(),
                    "A function definition requires a quoted string name.".to_string(),
                    bad.span,
                ));
                self.recover_after_body_item();
                return None;
            }
            None => {
                self.diags.push(Diagnostic::error(
                    "Unexpected EOF".to_string(),
                    "Expected function name.".to_string(),
                    ident.span.clone(),
                ));
                return None;
            }
        };
        let name = name_tok.text.trim_matches('"').to_string();
        match self.advance() {
            Some(t) if t.kind == TokenKind::OBrace => {}
            Some(bad) => {
                self.diags.push(Diagnostic::error(
                    "Invalid function definition".to_string(),
                    "Expected opening brace `{` after function name.".to_string(),
                    bad.span,
                ));
                self.recover_after_body_item();
                return None;
            }
            None => {
                self.diags.push(Diagnostic::error(
                    "Unexpected EOF".to_string(),
                    "Expected opening brace `{` after function name.".to_string(),
                    ident.span.clone(),
                ));
                return None;
            }
        }
        let body = self.parse_body_inner(Some(&TokenKind::CBrace));
        let Some(close_tok) = self.advance() else {
            self.diags.push(Diagnostic::error(
                "Unexpected EOF".to_string(),
                "Expected closing brace `}` for function.".to_string(),
                ident.span.clone(),
            ));
            return None;
        };
        let close_brace_span = close_tok.span;
        let next = self.peek_kind();
        match next {
            Some(TokenKind::Newline) | None => {
                if next.is_some() {
                    self.advance();
                }
            }
            Some(TokenKind::CBrace) => {}
            _ => {
                let bad_span = self.current_span_or(ident.span.clone());
                self.diags.push(Diagnostic::error(
                    "Missing newline after function definition".to_string(),
                    "A function definition must end with a newline.".to_string(),
                    bad_span,
                ));
                self.recover_after_body_item();
            }
        }
        let Some(r_attr) = body.attributes.get("result") else {
            self.diags.push(Diagnostic::error(
                "Missing result".to_string(),
                "A function definition must have a `result = ...` attribute.".to_string(),
                close_brace_span,
            ));
            return None;
        };
        let result_expr = r_attr.expr.clone();
        let (params, return_type, variadic_param) = Self::extract_function_block_parts(&body);
        let span = ident.span.merge(&close_brace_span);
        Some(
            FunctionBlock::new(name, params, return_type, result_expr, span)
                .with_variadic_param(variadic_param),
        )
    }

    fn finish_parsing_dynamic_block(&mut self, ident: &Token) -> Option<DynamicBlock> {
        let (block_type, type_span) = match self.advance() {
            Some(t) if t.kind == TokenKind::String => {
                (t.text.trim_matches('"').to_string(), t.span)
            }
            Some(t) if t.kind == TokenKind::Ident => (t.text, t.span),
            Some(bad) => {
                self.diags.push(Diagnostic::error(
                    "Invalid dynamic block definition".to_string(),
                    "A dynamic block definition requires a label specifying the block type to generate.".to_string(),
                    bad.span,
                ));
                self.recover_after_body_item();
                return None;
            }
            None => {
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
                self.diags.push(Diagnostic::error(
                    "Block opening brace expected".to_string(),
                    "Expected '{' after dynamic block type label.".to_string(),
                    bad.span,
                ));
                self.recover_after_body_item();
                return None;
            }
            None => {
                self.diags.push(Diagnostic::error(
                    "Unexpected EOF".to_string(),
                    "Expected '{' to start dynamic block body.".to_string(),
                    type_span,
                ));
                return None;
            }
        };
        let inner_body = self.parse_body_inner(Some(&TokenKind::CBrace));
        let close_brace_span = if let Some(t) = self.advance() {
            t.span
        } else {
            self.diags.push(Diagnostic::error(
                "Unclosed block".to_string(),
                "Expected '}' to close dynamic block body.".to_string(),
                open_brace.span.clone(),
            ));
            open_brace.span
        };
        let span = ident.span.merge(&close_brace_span);
        let Some(for_each_attr) = inner_body.attributes.get("for_each") else {
            self.diags.push(Diagnostic::error(
                "Missing required argument".to_string(),
                "A dynamic block must contain a 'for_each' argument.".to_string(),
                span,
            ));
            return None;
        };
        let for_each = for_each_attr.expr.clone();
        let (iterator, labels) = self.extract_dynamic_block_options(&inner_body);
        let Some(content_block) = inner_body.blocks.iter().find(|b| b.block_type == "content")
        else {
            self.diags.push(Diagnostic::error(
                "Missing required block",
                "A dynamic block must contain a 'content' block.",
                span,
            ));
            return None;
        };
        Some(DynamicBlock::new(
            block_type,
            for_each,
            iterator,
            labels,
            content_block.body.clone(),
            span,
            type_span,
        ))
    }

    fn extract_dynamic_block_options(
        &mut self,
        inner_body: &Body,
    ) -> (Option<String>, Option<Vec<Expression>>) {
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
        (iterator, labels)
    }
    fn finish_parsing_assertion_block(
        &mut self,
        ident: &Token,
        block_type: &str,
    ) -> Option<(Expression, Expression, Span)> {
        let open_brace = self.advance_token();
        let inner_body = self.parse_body_inner(Some(&TokenKind::CBrace));
        let close_brace_span = if let Some(t) = self.advance() {
            t.span
        } else {
            #[rustfmt::skip]
            self.diags
                .push(
                    Diagnostic::error(
                        "Unclosed block".to_string(),
                        format!("Expected '}}' to close {block_type} block body."),
                        open_brace.span.clone(),
                    ),
                );
            open_brace.span
        };
        let span = ident.span.merge(&close_brace_span);
        let Some(condition_attr) = inner_body.attributes.get("condition") else {
            #[rustfmt::skip]
            self.diags
                .push(
                    Diagnostic::error(
                        "Missing required argument".to_string(),
                        format!(
                            "A {block_type} block must contain a 'condition' argument."
                        ),
                        span,
                    ),
                );
            return None;
        };
        let Some(error_msg_attr) = inner_body.attributes.get("error_message") else {
            #[rustfmt::skip]
            self.diags
                .push(
                    Diagnostic::error(
                        "Missing required argument".to_string(),
                        format!(
                            "A {block_type} block must contain an 'error_message' argument."
                        ),
                        span,
                    ),
                );
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
    fn parse_block_labels_until_brace(
        &mut self,
        ident_span: &Span,
        labels: &mut Vec<String>,
        label_spans: &mut Vec<Span>,
    ) -> Option<Token> {
        loop {
            match self.peek_kind() {
                Some(TokenKind::OBrace) => return Some(self.advance_token()),
                Some(TokenKind::String) => {
                    let tok = self.advance_token();
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
                    let tok = self.advance_token();
                    labels.push(tok.text);
                    label_spans.push(tok.span);
                }
                Some(TokenKind::Assign) => {
                    let bad = self.advance_token();
                    self.diags.push(Diagnostic::error(
                        "Invalid block definition".to_string(),
                        "The equals sign \"=\" indicates an argument definition, and must not be used when defining a block.".to_string(),
                        bad.span,
                    ));
                    self.recover_after_body_item();
                    return None;
                }
                Some(TokenKind::Newline) => {
                    let bad = self.advance_token();
                    self.diags.push(Diagnostic::error(
                        "Invalid block definition".to_string(),
                        "A block definition must have block content delimited by \"{\" and \"}\", starting on the same line as the block header.".to_string(),
                        bad.span,
                    ));
                    self.recover_after_body_item();
                    return None;
                }
                Some(_) | None => {
                    let span = self
                        .current
                        .as_ref()
                        .map_or_else(|| ident_span.clone(), |t| t.span.clone());
                    self.diags.push(Diagnostic::error(
                        "Invalid block definition".to_string(),
                        "Either a quoted string block label or an opening brace (\"{\") is expected here.".to_string(),
                        span,
                    ));
                    self.recover_after_body_item();
                    return None;
                }
            }
        }
    }

    fn finish_parsing_block(
        &mut self,
        ident: Token,
        leading_comments: Vec<String>,
    ) -> Option<Block> {
        let mut labels = Vec::new();
        let mut label_spans = Vec::new();
        let open_brace =
            self.parse_block_labels_until_brace(&ident.span, &mut labels, &mut label_spans)?;
        let body = self.parse_body_inner(Some(&TokenKind::CBrace));
        let close_brace_span = if let Some(cbrace) = self.advance() {
            cbrace.span
        } else {
            let span = open_brace.span.clone();
            self.diags.push(Diagnostic::error(
                "Unclosed configuration block".to_string(),
                "There is no closing brace for this block before the end of the file.".to_string(),
                span.clone(),
            ));
            span
        };
        let trailing_comment = self.trailing_comment.take();
        let next = self.peek_kind();
        match next {
            Some(TokenKind::Newline) | None => {
                if next.is_some() {
                    self.advance();
                }
            }
            Some(TokenKind::CBrace) => {}
            _ => {
                let bad_span = self.current_span_or(ident.span.clone());
                self.diags.push(Diagnostic::error(
                    "Missing newline after block definition".to_string(),
                    "A block definition must end with a newline.".to_string(),
                    bad_span,
                ));
                self.recover_after_body_item();
            }
        }
        Some(Block {
            block_type: ident.text,
            labels,
            body,
            span: ident.span.merge(&close_brace_span),
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
                    break;
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
            let infix_tok = self.advance_token();
            left = self.parse_infix(left, &infix_tok)?;
        }
        Some(left)
    }
    fn parse_for_header(
        &mut self,
        comp_kind: &str,
    ) -> Option<(Option<String>, String, Expression)> {
        self.advance();
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let first_var = self.advance_token();
        if first_var.kind != TokenKind::Ident {
            self.diags.push(Diagnostic::error(
                "Expected identifier".to_string(),
                format!("Expected identifier after 'for' in {comp_kind} comprehension."),
                first_var.span,
            ));
            return None;
        }
        let (key_var, val_var) = if self.peek_kind() == Some(TokenKind::Comma) {
            self.advance();
            while self.peek_kind() == Some(TokenKind::Newline) {
                self.advance();
            }
            let second_var = self.advance_token();
            if second_var.kind != TokenKind::Ident {
                self.diags.push(Diagnostic::error(
                    "Expected identifier".to_string(),
                    "Expected second identifier after ',' in for expression.".to_string(),
                    second_var.span,
                ));
                return None;
            }
            (Some(first_var.text), second_var.text)
        } else {
            (None, first_var.text)
        };
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let in_tok = self.advance_token();
        if in_tok.kind != TokenKind::Ident || in_tok.text != "in" {
            self.diags.push(Diagnostic::error(
                "Expected `in`".to_string(),
                "Expected `in` after iterator variable in for expression.".to_string(),
                in_tok.span,
            ));
            return None;
        }
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let collection = self.parse_expression()?;
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        Some((key_var, val_var, collection))
    }

    fn parse_for_tuple(&mut self, open_span: &Span) -> Option<Expression> {
        let (key_var, val_var, collection) = self.parse_for_header("tuple")?;
        let colon = self.advance_token();
        if colon.kind != TokenKind::Colon {
            self.diags.push(Diagnostic::error(
                "Expected `:`".to_string(),
                "Expected `:` to introduce value expression in for comprehension.".to_string(),
                colon.span,
            ));
            return None;
        }
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let val_expr = self.parse_expression()?;
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let cond_expr = if self
            .current
            .as_ref()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "if")
        {
            self.advance();
            while self.peek_kind() == Some(TokenKind::Newline) {
                self.advance();
            }
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let close_tok = self.advance_token();
        if close_tok.kind != TokenKind::CBrack {
            self.diags.push(Diagnostic::error(
                "Expected `]`".to_string(),
                "Expected `]` to close tuple comprehension.".to_string(),
                close_tok.span,
            ));
            return None;
        }
        let span = Span::new(
            open_span.start_byte,
            close_tok.span.end_byte,
            open_span.start_line,
            open_span.start_col,
            close_tok.span.end_line,
            close_tok.span.end_col,
        );
        Some(Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var,
                val_var,
                collection: Box::new(collection),
                key_expr: None,
                val_expr: Box::new(val_expr),
                cond_expr,
                grouping: false,
            }),
            span,
        ))
    }

    fn parse_for_object(&mut self, open_span: &Span) -> Option<Expression> {
        let (key_var, val_var, collection) = self.parse_for_header("object")?;
        let colon = self.advance_token();
        if colon.kind != TokenKind::Colon {
            self.diags.push(Diagnostic::error(
                "Expected `:`".to_string(),
                "Expected `:` to introduce key expression in object comprehension.".to_string(),
                colon.span,
            ));
            return None;
        }
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let key_expr = self.parse_expression()?;
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let arrow = self.advance_token();
        if arrow.kind != TokenKind::FatArrow {
            self.diags.push(Diagnostic::error(
                "Expected `=>`".to_string(),
                "Expected `=>` between key and value expressions in object comprehension."
                    .to_string(),
                arrow.span,
            ));
            return None;
        }
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let val_expr = self.parse_expression()?;
        let grouping = if self.peek_kind() == Some(TokenKind::Ellipsis) {
            self.advance();
            true
        } else {
            false
        };
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let cond_expr = if self
            .current
            .as_ref()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "if")
        {
            self.advance();
            while self.peek_kind() == Some(TokenKind::Newline) {
                self.advance();
            }
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let close_tok = self.advance_token();
        if close_tok.kind != TokenKind::CBrace {
            self.diags.push(Diagnostic::error(
                "Expected `}`".to_string(),
                "Expected `}` to close object comprehension.".to_string(),
                close_tok.span,
            ));
            return None;
        }
        let span = Span::new(
            open_span.start_byte,
            close_tok.span.end_byte,
            open_span.start_line,
            open_span.start_col,
            close_tok.span.end_line,
            close_tok.span.end_col,
        );
        Some(Expression::ForExpr(
            Box::new(crate::ast::expr::ForExpr {
                key_var,
                val_var,
                collection: Box::new(collection),
                key_expr: Some(Box::new(key_expr)),
                val_expr: Box::new(val_expr),
                cond_expr,
                grouping,
            }),
            span,
        ))
    }
    /// Parses the content of a string or heredoc into template parts.
    pub(crate) fn parse_template(
        diags: &mut Diagnostics,
        text: &str,
        span: &Span,
    ) -> Vec<crate::ast::expr::TemplatePart> {
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
        let mut raw_items =
            tokenize_template_raw_items(diags, &normalized_content, is_heredoc, span);
        apply_template_strip_markers(&mut raw_items);
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
                if template_parts.len() == 1 {
                    if let crate::ast::expr::TemplatePart::Literal(ref s, _) = template_parts[0] {
                        if tok.kind == TokenKind::String {
                            return Some(Expression::String(s.clone(), tok.span));
                        }
                    }
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
            TokenKind::Ident => self.parse_prefix_ident(tok),
            TokenKind::OParen => self.parse_prefix_paren(&tok.span),
            TokenKind::OBrack => self.parse_prefix_tuple(&tok.span),
            TokenKind::OBrace => self.parse_prefix_object(&tok.span),
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
                self.diags.push(Diagnostic::error(
                    "Expected expression".to_string(),
                    "An expression was expected here.".to_string(),
                    tok.span,
                ));
                None
            }
        }
    }

    fn parse_prefix_ident(&mut self, tok: Token) -> Option<Expression> {
        let mut name = tok.text;
        let mut span = tok.span;
        let mut has_ns = false;
        while self.peek_kind() == Some(TokenKind::ColonColon) {
            has_ns = true;
            let cc_tok = self.advance_token();
            span = span.merge(&cc_tok.span);
            if self.peek_kind() == Some(TokenKind::Ident) {
                let next_ident = self.advance_token();
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

    fn parse_prefix_paren(&mut self, open_span: &Span) -> Option<Expression> {
        self.paren_depth += 1;
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let inner = self.parse_expression();
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        self.paren_depth = self.paren_depth.saturating_sub(1);
        let inner = inner?;
        let cparen = self.advance_token();
        if cparen.kind != TokenKind::CParen {
            self.diags.push(Diagnostic::error(
                "Expected closing parenthesis".to_string(),
                "Expected `)` to close the expression.".to_string(),
                cparen.span,
            ));
            return None;
        }
        let span = Span::new(
            open_span.start_byte,
            cparen.span.end_byte,
            open_span.start_line,
            open_span.start_col,
            cparen.span.end_line,
            cparen.span.end_col,
        );
        Some(Expression::Parentheses(Box::new(inner), span))
    }

    fn parse_prefix_tuple(&mut self, open_span: &Span) -> Option<Expression> {
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        if self
            .current
            .as_ref()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "for")
        {
            return self.parse_for_tuple(open_span);
        }
        let mut elements = Vec::new();
        let end_span = loop {
            if self.peek_kind() == Some(TokenKind::CBrack) {
                break self.advance_token().span;
            }
            if self.peek_kind() == Some(TokenKind::Newline) {
                self.advance();
                continue;
            }
            let expr = self.parse_expression()?;
            elements.push(expr);
            match self.peek_kind() {
                Some(TokenKind::Comma | TokenKind::Newline) => {
                    self.advance();
                }
                Some(TokenKind::CBrack) => {
                    break self.advance_token().span;
                }
                _ => {
                    let bad = self.advance_token();
                    self.diags.push(Diagnostic::error(
                        "Expected comma or `]`".to_string(),
                        "Expected a comma to separate tuple elements, or `]` to close the tuple."
                            .to_string(),
                        bad.span,
                    ));
                    return None;
                }
            }
        };
        let span = Span::new(
            open_span.start_byte,
            end_span.end_byte,
            open_span.start_line,
            open_span.start_col,
            end_span.end_line,
            end_span.end_col,
        );
        Some(Expression::Tuple(elements, span))
    }

    fn parse_prefix_object(&mut self, open_span: &Span) -> Option<Expression> {
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        if self
            .current
            .as_ref()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "for")
        {
            return self.parse_for_object(open_span);
        }
        let mut elements = Vec::new();
        let end_span = loop {
            if self.peek_kind() == Some(TokenKind::CBrace) {
                break self.advance_token().span;
            }
            if self.peek_kind() == Some(TokenKind::Newline) {
                self.advance();
                continue;
            }
            let key = self.parse_expression()?;
            let op = self.advance_token();
            if op.kind != TokenKind::Assign && op.kind != TokenKind::Colon {
                self.diags.push(Diagnostic::error(
                    "Expected `=` or `:`".to_string(),
                    "Expected `=` or `:` in object constructor.".to_string(),
                    op.span,
                ));
                return None;
            }
            let val = self.parse_expression()?;
            elements.push((key, val));
            match self.peek_kind() {
                Some(TokenKind::Comma | TokenKind::Newline) => {
                    self.advance();
                }
                Some(TokenKind::CBrace) => {
                    break self.advance_token().span;
                }
                _ => {
                    let bad = self.advance_token();
                    self.diags.push(Diagnostic::error(
                        "Expected comma, newline, or `}`".to_string(),
                        "Expected a separator or `}` in object constructor.".to_string(),
                        bad.span,
                    ));
                    return None;
                }
            }
        };
        let span = Span::new(
            open_span.start_byte,
            end_span.end_byte,
            open_span.start_line,
            open_span.start_col,
            end_span.end_line,
            end_span.end_col,
        );
        Some(Expression::Object(elements, span))
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
            | TokenKind::Or => self.parse_infix_binary(left, tok),
            TokenKind::Question => self.parse_infix_conditional(left),
            TokenKind::Dot => self.parse_infix_dot(left, tok),
            TokenKind::OBrack => self.parse_infix_index(left, tok),
            _ => self.parse_infix_call(&left),
        }
    }

    fn parse_infix_binary(&mut self, left: Expression, tok: &Token) -> Option<Expression> {
        let prec = crate::parse::Precedence::from_token(&tok.kind);
        while self.paren_depth > 0 && self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
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

    fn parse_infix_conditional(&mut self, left: Expression) -> Option<Expression> {
        while self.paren_depth > 0 && self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let true_expr = self.parse_expression()?;
        while self.paren_depth > 0 && self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let colon = self.advance()?;
        if colon.kind != TokenKind::Colon {
            self.diags.push(Diagnostic::error(
                "Expected `:`".to_string(),
                "Expected `:` for the conditional expression.".to_string(),
                colon.span,
            ));
            return None;
        }
        while self.paren_depth > 0 && self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let false_expr = self.parse_expression_precedence(crate::parse::Precedence::Conditional)?;
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

    fn parse_infix_dot(&mut self, left: Expression, tok: &Token) -> Option<Expression> {
        let attr_tok = self.advance()?;
        let op = match attr_tok.kind {
            TokenKind::Ident => {
                crate::ast::expr::TraversalOperator::GetAttr(attr_tok.text, attr_tok.span.clone())
            }
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
                    self.diags.push(Diagnostic::error(
                        "Expected integer index".to_string(),
                        "Expected an integer for legacy index after `.`.".to_string(),
                        attr_tok.span,
                    ));
                    return None;
                }
            }
            _ => {
                self.diags.push(Diagnostic::error(
                    "Expected identifier".to_string(),
                    "Expected attribute name after `.`.".to_string(),
                    attr_tok.span,
                ));
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

    fn parse_infix_index(&mut self, left: Expression, tok: &Token) -> Option<Expression> {
        let (op, cbrack_span) = if self.peek_kind() == Some(TokenKind::Star) {
            let _star_tok = self.advance_token();
            let Some(cbrack) = self.advance() else {
                self.diags.push(Diagnostic::error(
                    "Expected `]`".to_string(),
                    "Expected `]` to close the full splat operator.".to_string(),
                    tok.span.clone(),
                ));
                return None;
            };
            if cbrack.kind != TokenKind::CBrack {
                self.diags.push(Diagnostic::error(
                    "Expected `]`".to_string(),
                    "Expected `]` to close the full splat operator.".to_string(),
                    cbrack.span,
                ));
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
                self.diags.push(Diagnostic::error(
                    "Expected `]`".to_string(),
                    "Expected `]` to close the index expression.".to_string(),
                    tok.span.clone(),
                ));
                return None;
            };
            if cbrack.kind != TokenKind::CBrack {
                self.diags.push(Diagnostic::error(
                    "Expected `]`".to_string(),
                    "Expected `]` to close the index expression.".to_string(),
                    cbrack.span,
                ));
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

    fn parse_infix_call(&mut self, left: &Expression) -> Option<Expression> {
        let mut args = Vec::new();
        while self.peek_kind() == Some(TokenKind::Newline) {
            self.advance();
        }
        let (cparen_tok, expand_final) = if self.peek_kind() == Some(TokenKind::CParen) {
            (self.advance_token(), false)
        } else {
            let mut expand = false;
            let cparen = loop {
                let expr = self.parse_expression()?;
                args.push(expr);
                while self.peek_kind() == Some(TokenKind::Newline) {
                    self.advance();
                }
                match self.peek_kind() {
                    Some(TokenKind::Comma) => {
                        self.advance();
                        while self.peek_kind() == Some(TokenKind::Newline) {
                            self.advance();
                        }
                        if self.peek_kind() == Some(TokenKind::CParen) {
                            break self.advance_token();
                        }
                    }
                    Some(TokenKind::Ellipsis) => {
                        self.advance();
                        expand = true;
                        while self.peek_kind() == Some(TokenKind::Newline) {
                            self.advance();
                        }
                        let next = self.advance_token();
                        if next.kind != TokenKind::CParen {
                            self.diags.push(Diagnostic::error(
                                "Expected `)`".to_string(),
                                "Expected `)` after `...`.".to_string(),
                                next.span,
                            ));
                            return None;
                        }
                        break next;
                    }
                    Some(TokenKind::CParen) => {
                        break self.advance_token();
                    }
                    _ => {
                        let bad = self.advance_token();
                        self.diags.push(Diagnostic::error(
                            "Expected `,` or `)`".to_string(),
                            "Expected a comma to separate function arguments, or `)` to close the call.".to_string(),
                            bad.span,
                        ));
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
        let name = if let Expression::Variable(ref name, _) = *left {
            crate::ast::expr::NamespacedIdent::parse(name, left.span())
        } else {
            self.diags.push(Diagnostic::error(
                "Invalid function call".to_string(),
                "Only top-level identifiers can be called as functions.".to_string(),
                left.span(),
            ));
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests_template_directives_edge_cases {
    use super::*;
    use crate::error::HclError;

    #[test]
    fn test_parse_template_escapes_edge_cases() -> Result<(), HclError> {
        let input = "\"hello $${ world %%{foo\"";
        let mut p = Parser::new(input);
        let expr = p
            .parse_expression()
            .ok_or_else(|| HclError::Parse("expected expression".to_string()))?;
        // Escaped interpolations don't break out of the string token, so it parses as a simple string.
        assert!(matches!(expr, crate::ast::expr::Expression::String(_, _)));
        Ok(())
    }

    #[test]
    fn test_parse_template_escapes_edge_cases_2() {
        let input = "\"$$%%{\"";
        let mut p = Parser::new(input);
        assert!(p.parse_expression().is_some());
    }
}
