//! Lexer for legacy HCL 1.0 syntax.
//!
//! Handles tokenization of HCL 1.0 documents, including bare identifiers,
//! legacy comment styles (`#`, `//`, `/* ... */`), and heredocs with `${}` interpolations.
use crate::error::HclError;
use crate::hcl1::token::{Hcl1Token, Hcl1TokenKind};
use crate::span::Span;
/// Lexer for tokenizing HCL 1.0 input streams.
#[derive(Debug)]
pub struct Hcl1Lexer<'a> {
    source: &'a str,
    chars: Vec<(usize, char)>,
    cursor: usize,
    line: usize,
    col: usize,
}
impl<'a> Hcl1Lexer<'a> {
    /// Creates a new [`Hcl1Lexer`] for the given input string.
    ///
    /// # Arguments
    /// * `source` - The HCL 1.0 source string.
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        let chars: Vec<(usize, char)> = source.char_indices().collect();
        Self {
            source,
            chars,
            cursor: 0,
            line: 1,
            col: 1,
        }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).map(|(_, c)| *c)
    }
    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.cursor + 1).map(|(_, c)| *c)
    }
    fn advance(&mut self) -> Option<char> {
        if let Some(&(_, c)) = self.chars.get(self.cursor) {
            self.cursor += 1;
            if c == '\x0a' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            Some(c)
        } else {
            None
        }
    }
    fn current_byte(&self) -> usize {
        self.chars
            .get(self.cursor)
            .map_or(self.source.len(), |&(idx, _)| idx)
    }
    /// Tokenizes the entire input into a sequence of [`Hcl1Token`]s.
    ///
    /// # Errors
    /// Returns [`HclError::Lex`] if invalid or unclosed tokens are encountered.
    pub fn tokenize(&mut self) -> Result<Vec<Hcl1Token>, HclError> {
        let mut tokens = Vec::new();
        while let Some(c) = self.peek() {
            let start_byte = self.current_byte();
            let start_line = self.line;
            let start_col = self.col;
            if c == ' ' || c == '\x09' || c == '\x0d' {
                let mut text = String::new();
                while let Some(ch) = self.peek() {
                    if ch == ' ' || ch == '\x09' || ch == '\x0d' {
                        text.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                }
                let end_byte = self.current_byte();
                let span = Span::new(
                    start_byte, end_byte, start_line, start_col, self.line, self.col,
                );
                tokens.push(Hcl1Token::new(Hcl1TokenKind::Whitespace, text, span));
                continue;
            }
            if c == '\x0a' {
                self.advance();
                let end_byte = self.current_byte();
                let span = Span::new(
                    start_byte, end_byte, start_line, start_col, self.line, self.col,
                );
                tokens.push(Hcl1Token::new(Hcl1TokenKind::Newline, "\n", span));
                continue;
            }
            if c == '#' {
                let mut text = String::new();
                while let Some(ch) = self.peek() {
                    if ch == '\x0a' {
                        break;
                    }
                    text.push(ch);
                    self.advance();
                }
                let end_byte = self.current_byte();
                let span = Span::new(
                    start_byte, end_byte, start_line, start_col, self.line, self.col,
                );
                tokens.push(Hcl1Token::new(Hcl1TokenKind::Comment, text, span));
                continue;
            }
            if c == '/' && self.peek_next() == Some('/') {
                let mut text = String::new();
                while let Some(ch) = self.peek() {
                    if ch == '\x0a' {
                        break;
                    }
                    text.push(ch);
                    self.advance();
                }
                let end_byte = self.current_byte();
                let span = Span::new(
                    start_byte, end_byte, start_line, start_col, self.line, self.col,
                );
                tokens.push(Hcl1Token::new(Hcl1TokenKind::Comment, text, span));
                continue;
            }
            if c == '/' && self.peek_next() == Some('*') {
                let mut text = String::new();
                text.push(self.advance().unwrap_or('/'));
                text.push(self.advance().unwrap_or('*'));
                while let Some(ch) = self.advance() {
                    text.push(ch);
                    if ch == '*' && self.peek() == Some('/') {
                        text.push(self.advance().unwrap_or('/'));
                        break;
                    }
                }
                let end_byte = self.current_byte();
                let span = Span::new(
                    start_byte, end_byte, start_line, start_col, self.line, self.col,
                );
                tokens.push(Hcl1Token::new(Hcl1TokenKind::Comment, text, span));
                continue;
            }
            if c == '<' && self.peek_next() == Some('<') {
                let token = self.lex_heredoc(start_byte, start_line, start_col)?;
                tokens.push(token);
                continue;
            }
            let punct = match c {
                '=' => Some(Hcl1TokenKind::Assign),
                ':' => Some(Hcl1TokenKind::Colon),
                ',' => Some(Hcl1TokenKind::Comma),
                '.' => Some(Hcl1TokenKind::Dot),
                '{' => Some(Hcl1TokenKind::OBrace),
                '}' => Some(Hcl1TokenKind::CBrace),
                '[' => Some(Hcl1TokenKind::OBrack),
                ']' => Some(Hcl1TokenKind::CBrack),
                '(' => Some(Hcl1TokenKind::OParen),
                ')' => Some(Hcl1TokenKind::CParen),
                _ => None,
            };
            if let Some(kind) = punct {
                self.advance();
                let end_byte = self.current_byte();
                let span = Span::new(
                    start_byte, end_byte, start_line, start_col, self.line, self.col,
                );
                tokens.push(Hcl1Token::new(kind, c.to_string(), span));
                continue;
            }
            if c == '"' {
                let token = self.lex_string(start_byte, start_line, start_col)?;
                tokens.push(token);
                continue;
            }
            if c.is_ascii_digit()
                || (c == '-' && self.peek_next().is_some_and(|n| n.is_ascii_digit()))
            {
                let token = self.lex_number(start_byte, start_line, start_col);
                tokens.push(token);
                continue;
            }
            if c.is_ascii_alphabetic() || c == '_' {
                let token = self.lex_ident(start_byte, start_line, start_col);
                tokens.push(token);
                continue;
            }
            let ch = self.advance().unwrap_or(c);
            return Err(HclError::Lex(format!(
                "Unexpected character '{ch}' at line {start_line}, col {start_col}"
            )));
        }
        Ok(tokens)
    }
    fn lex_string(
        &mut self,
        start_byte: usize,
        start_line: usize,
        start_col: usize,
    ) -> Result<Hcl1Token, HclError> {
        self.advance();
        let mut text = String::new();
        let mut closed = false;
        while let Some(ch) = self.advance() {
            if ch == '"' {
                closed = true;
                break;
            } else if ch == '\x5c' {
                if let Some(escaped) = self.advance() {
                    match escaped {
                        'n' => text.push('\x0a'),
                        't' => text.push('\x09'),
                        'r' => text.push('\x0d'),
                        '\x5c' => text.push('\x5c'),
                        '"' => text.push('"'),
                        other => {
                            text.push('\x5c');
                            text.push(other);
                        }
                    }
                }
            } else {
                text.push(ch);
            }
        }
        if !closed {
            return Err(HclError::Lex(format!(
                "Unterminated string literal starting at line {start_line}, col {start_col}"
            )));
        }
        let end_byte = self.current_byte();
        let span = Span::new(
            start_byte, end_byte, start_line, start_col, self.line, self.col,
        );
        Ok(Hcl1Token::new(Hcl1TokenKind::String, text, span))
    }
    fn lex_number(&mut self, start_byte: usize, start_line: usize, start_col: usize) -> Hcl1Token {
        let mut text = String::new();
        if self.peek() == Some('-') {
            text.push(self.advance().unwrap_or('-'));
        }
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' || ch == '+' || ch == '-'
            {
                text.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        let end_byte = self.current_byte();
        let span = Span::new(
            start_byte, end_byte, start_line, start_col, self.line, self.col,
        );
        Hcl1Token::new(Hcl1TokenKind::Number, text, span)
    }
    fn lex_ident(&mut self, start_byte: usize, start_line: usize, start_col: usize) -> Hcl1Token {
        let mut text = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                text.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        let kind = match text.as_str() {
            "true" | "false" => Hcl1TokenKind::Bool,
            "null" => Hcl1TokenKind::Null,
            _ => Hcl1TokenKind::Ident,
        };
        let end_byte = self.current_byte();
        let span = Span::new(
            start_byte, end_byte, start_line, start_col, self.line, self.col,
        );
        Hcl1Token::new(kind, text, span)
    }
    fn lex_heredoc(
        &mut self,
        start_byte: usize,
        start_line: usize,
        start_col: usize,
    ) -> Result<Hcl1Token, HclError> {
        self.advance();
        self.advance();
        let is_indented = self.peek() == Some('-');
        if is_indented {
            self.advance();
        }
        let mut marker = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                marker.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if marker.is_empty() {
            return Err(HclError::Lex(format!(
                "Invalid heredoc delimiter at line {start_line}, col {start_col}"
            )));
        }
        while let Some(ch) = self.peek() {
            self.advance();
            if ch == '\x0a' {
                break;
            }
        }
        let mut content = String::new();
        let mut closed = false;
        loop {
            let mut line = String::new();
            while let Some(ch) = self.advance() {
                if ch == '\x0a' {
                    break;
                }
                line.push(ch);
            }
            let trimmed = if is_indented { line.trim() } else { &line };
            if trimmed == marker {
                closed = true;
                break;
            }
            content.push_str(&line);
            content.push('\x0a');
            if self.cursor >= self.chars.len() {
                break;
            }
        }
        if !closed {
            return Err(HclError::Lex(format!(
                "Unterminated heredoc `{marker}` starting at line {start_line}, col {start_col}"
            )));
        }
        let end_byte = self.current_byte();
        let span = Span::new(
            start_byte, end_byte, start_line, start_col, self.line, self.col,
        );
        Ok(Hcl1Token::new(Hcl1TokenKind::Heredoc, content, span))
    }
}
