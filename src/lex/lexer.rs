//! The actual lexer wrapper over logos.

use crate::lex::token::{Token, TokenKind};
use crate::span::Span;
use logos::Logos;

/// A lexical error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    /// The span of the error.
    pub span: crate::span::Span,
    /// The unrecognized text.
    pub text: String,
}

/// A Lexer for HCL that emits spanned Tokens.
pub struct Lexer<'a> {
    inner: logos::Lexer<'a, TokenKind>,
    line_starts: Vec<usize>,
    file: Option<std::sync::Arc<str>>,
    source_len: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given input string without a file identifier.
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self::new_with_file(input, None)
    }

    /// Create a new lexer for the given input string with an optional source file origin.
    ///
    /// # Arguments
    /// * `input` - The source text to tokenize.
    /// * `file` - Optional source file identifier.
    #[must_use]
    pub fn new_with_file(input: &'a str, file: Option<std::sync::Arc<str>>) -> Self {
        // Pre-calculate line starts for accurate row/col tracking
        let mut line_starts = vec![0];
        for (i, c) in input.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }

        Self {
            inner: TokenKind::lexer(input),
            line_starts,
            file,
            source_len: input.len(),
        }
    }

    /// Calculate the (line, col) for a given byte offset.
    /// Lines and columns are 1-indexed.
    fn line_col(&self, byte_offset: usize) -> (usize, usize) {
        match self.line_starts.binary_search(&byte_offset) {
            Ok(idx) => (idx + 1, 1),
            Err(idx) => {
                let line = idx;
                let line_start = self.line_starts[idx - 1];
                let col = byte_offset - line_start + 1;
                (line, col)
            }
        }
    }

    /// Attempt to parse a heredoc manually from the current position.
    fn parse_heredoc(&mut self) -> Option<Token> {
        let remainder = self.inner.remainder();
        if !remainder.starts_with("<<") {
            return None;
        }

        // We have a potential heredoc. Let's find the marker.
        let is_indented = remainder.starts_with("<<-");
        let marker_start_offset = if is_indented { 3 } else { 2 };

        // Find the end of the marker string on the FIRST line
        let first_line_end = remainder.find('\n').unwrap_or(remainder.len());
        let marker_str = remainder[marker_start_offset..first_line_end].trim();

        if marker_str.is_empty() {
            return None; // Invalid, let standard lexer deal with it or fail.
        }

        // Now look for the terminating marker on a subsequent line
        let mut search_offset = first_line_end;
        if search_offset < remainder.len() {
            search_offset += 1;
        }

        let mut heredoc_end = None;

        while search_offset < remainder.len() {
            // Find where this line ends
            let line_end_offset = remainder[search_offset..]
                .find('\n')
                .unwrap_or(remainder[search_offset..].len());
            let next_line_end = search_offset + line_end_offset;

            let line_content = &remainder[search_offset..next_line_end];
            let check_content = if is_indented {
                line_content.trim_start()
            } else {
                line_content
            };

            if check_content.trim_end() == marker_str {
                // The heredoc includes the final marker, so its end is after the marker.
                // We'll consider the heredoc ending exactly after the marker word.
                heredoc_end = Some(next_line_end);
                break;
            }

            // Advance search to the character after this newline
            search_offset = next_line_end + 1;
        }

        if let Some(end) = heredoc_end {
            let full_match = &remainder[..end];

            let span_start = self.source_len - remainder.len();
            self.inner.bump(end);
            let span_end = span_start + end;

            let (start_line, start_col) = self.line_col(span_start);
            let (end_line, end_col) = self.line_col(span_end);

            let span = Span::new_with_file(
                span_start,
                span_end,
                start_line,
                start_col,
                end_line,
                end_col,
                self.file.clone(),
            );

            return Some(Token::new(TokenKind::Heredoc, full_match.to_string(), span));
        }

        None
    }

    /// Attempt to parse a quoted string literal, correctly handling nested interpolations
    /// `${ ... }` and directives `%{ ... }` that may contain nested quotes and braces.
    fn parse_quoted_string(&mut self) -> Option<Token> {
        let remainder = self.inner.remainder();
        let bytes = remainder.as_bytes();
        if bytes.is_empty() || bytes[0] != b'"' {
            return None;
        }

        let mut idx = 1;
        let mut interp_depth = 0;

        while idx < bytes.len() {
            let b = bytes[idx];
            if b == b'\\' {
                idx += 1;
                if idx < bytes.len() {
                    idx += 1;
                }
                continue;
            }

            if interp_depth > 0 {
                if b == b'"' {
                    // Inner string inside interpolation: scan until closing quote
                    idx += 1;
                    while idx < bytes.len() {
                        if bytes[idx] == b'\\' {
                            idx += 2;
                        } else if bytes[idx] == b'"' {
                            idx += 1;
                            break;
                        } else {
                            idx += 1;
                        }
                    }
                    continue;
                }
                if b == b'{' {
                    interp_depth += 1;
                    idx += 1;
                } else if b == b'}' {
                    interp_depth -= 1;
                    idx += 1;
                } else {
                    idx += 1;
                }
            } else if (b == b'$' || b == b'%') && idx + 1 < bytes.len() && bytes[idx + 1] == b'{' {
                // Check for escape: $${ or %%{
                if idx > 1 && bytes[idx - 1] == b && bytes[idx - 2] != b'\\' {
                    idx += 2;
                } else {
                    interp_depth = 1;
                    idx += 2;
                }
            } else if b == b'"' {
                idx += 1;
                let full_match = &remainder[..idx];
                let span_start = self.source_len - remainder.len();
                self.inner.bump(idx);
                let span_end = span_start + idx;

                let (start_line, start_col) = self.line_col(span_start);
                let (end_line, end_col) = self.line_col(span_end);

                let span = Span::new_with_file(
                    span_start,
                    span_end,
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                    self.file.clone(),
                );

                return Some(Token::new(TokenKind::String, full_match.to_string(), span));
            } else {
                idx += 1;
            }
        }

        None
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<Token, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        let remainder = self.inner.remainder();

        // Intercept Heredoc `<<` parsing before standard lexer handles `<`
        if remainder.starts_with("<<")
            && let Some(heredoc_token) = self.parse_heredoc()
        {
            return Some(Ok(heredoc_token));
        }

        // Intercept Quoted Strings `"` to handle nested interpolations with quotes
        if remainder.starts_with('"')
            && let Some(str_token) = self.parse_quoted_string()
        {
            return Some(Ok(str_token));
        }

        let res = self.inner.next()?;
        let span_logos = self.inner.span();
        let text = self.inner.slice().to_string();

        let (start_line, start_col) = self.line_col(span_logos.start);
        let (end_line, end_col) = self.line_col(span_logos.end);

        let span = Span::new_with_file(
            span_logos.start,
            span_logos.end,
            start_line,
            start_col,
            end_line,
            end_col,
            self.file.clone(),
        );

        match res {
            Ok(kind) => Some(Ok(Token::new(kind, text, span))),
            Err(()) => Some(Err(LexError { span, text })),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unnecessary_wraps)]
    use super::*;

    fn next_tok(lex: &mut Lexer<'_>) -> Result<Token, crate::error::HclError> {
        lex.next()
            .ok_or_else(|| crate::error::HclError::Lex("EOF".to_string()))?
            .map_err(|e| crate::error::HclError::Lex(e.text.clone()))
    }

    #[test]
    fn test_lexer() -> Result<(), crate::error::HclError> {
        let input = "var = 123\n# comment\nfoo";
        let mut lex = Lexer::new(input);

        let t1 = next_tok(&mut lex)?;
        assert_eq!(t1.kind, TokenKind::Ident);
        assert!(t1.text.contains("var"));
        assert_eq!(t1.span.start_line, 1);
        assert_eq!(t1.span.start_col, 1);

        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Whitespace);

        let t2 = next_tok(&mut lex)?;
        assert_eq!(t2.kind, TokenKind::Assign);
        assert_eq!(t2.span.start_line, 1);
        assert_eq!(t2.span.start_col, 5);

        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Whitespace);

        let t3 = next_tok(&mut lex)?;
        assert_eq!(t3.kind, TokenKind::Number);
        assert!(t3.text.contains("123"));

        let tnl1 = next_tok(&mut lex)?;
        assert_eq!(tnl1.kind, TokenKind::Newline);

        let tc = next_tok(&mut lex)?;
        assert_eq!(tc.kind, TokenKind::InlineComment);

        let tnl2 = next_tok(&mut lex)?;
        assert_eq!(tnl2.kind, TokenKind::Newline); // From after the comment

        let t4 = next_tok(&mut lex)?;
        assert_eq!(t4.kind, TokenKind::Ident);
        assert!(t4.text.contains("foo"));
        assert_eq!(t4.span.start_line, 3);
        assert_eq!(t4.span.start_col, 1);

        assert!(lex.next().is_none());
        Ok(())
    }

    #[test]
    fn test_lexer_line_col_exact() -> Result<(), crate::error::HclError> {
        let input = "a\nb";
        let mut lex = Lexer::new(input);

        let t1 = next_tok(&mut lex)?;
        assert_eq!(t1.span.start_line, 1);
        assert_eq!(t1.span.start_col, 1);
        assert_eq!(t1.span.end_line, 1);
        assert_eq!(t1.span.end_col, 2);

        let tnl = next_tok(&mut lex)?;
        assert_eq!(tnl.kind, TokenKind::Newline);

        let t2 = next_tok(&mut lex)?;
        assert_eq!(t2.span.start_line, 2);
        assert_eq!(t2.span.start_col, 1);
        assert_eq!(t2.span.end_line, 2);
        assert_eq!(t2.span.end_col, 2);
        Ok(())
    }

    #[test]
    fn test_lexer_error() -> Result<(), crate::error::HclError> {
        // an invalid token that doesn't match any rule (e.g. backtick which is not in HCL)
        let input = "`";
        let mut lex = Lexer::new(input);
        let res = lex.next().unwrap_or(Err(LexError {
            span: crate::span::Span::default(),
            text: String::new(),
        }));
        assert!(res.is_err());
        Ok(())
    }

    #[test]
    fn test_heredoc_standard() -> Result<(), crate::error::HclError> {
        // We added a newline before the heredoc marker to make sure it matches properly on standard
        // trailing whitespace behavior.
        let input = "a = <<EOF\nhello\nEOF\nb = 2";
        let mut lex = Lexer::new(input);

        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Ident);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Whitespace);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Assign);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Whitespace);

        let hd = next_tok(&mut lex)?;
        assert_eq!(hd.kind, TokenKind::Heredoc);
        assert!(hd.text.contains("<<EOF\nhello\nEOF"));

        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Newline);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Ident);
        Ok(())
    }

    #[test]
    fn test_heredoc_indented() -> Result<(), crate::error::HclError> {
        let input = "<<-MARKER\n  indented\n  MARKER\n";
        let mut lex = Lexer::new(input);

        let hd = next_tok(&mut lex)?;
        assert_eq!(hd.kind, TokenKind::Heredoc);
        assert!(hd.text.contains("<<-MARKER\n  indented\n  MARKER"));
        Ok(())
    }

    #[test]
    fn test_heredoc_invalid_marker() -> Result<(), crate::error::HclError> {
        // If it starts with << but has no marker or doesn't end, it falls back to normal lexing
        // which will emit `<` and `<`
        let input = "<<\n";
        let mut lex = Lexer::new(input);

        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Lt);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Lt);
        Ok(())
    }

    #[test]
    fn test_heredoc_unclosed() -> Result<(), crate::error::HclError> {
        let input = "<<EOF\nnever closes";
        let mut lex = Lexer::new(input);

        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Lt);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Lt);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Ident); // EOF
        Ok(())
    }

    #[test]
    fn test_heredoc_no_newline() -> Result<(), crate::error::HclError> {
        // A heredoc token without a newline is not a valid heredoc, falls back to normal lexing
        let input = "<<EOF";
        let mut lex = Lexer::new(input);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Lt);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Lt);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Ident);
        Ok(())
    }

    #[test]
    fn test_not_heredoc_but_starts_with_lt() -> Result<(), crate::error::HclError> {
        let input = "<";
        let mut lex = Lexer::new(input);
        assert_eq!(next_tok(&mut lex)?.kind, TokenKind::Lt);
        Ok(())
    }

    #[test]
    fn test_heredoc_parser_bails_if_not_starts_with_lt_lt() -> Result<(), crate::error::HclError> {
        let input = "abc";
        let mut lex = Lexer::new(input);
        assert_eq!(lex.parse_heredoc(), None);
        Ok(())
    }

    #[test]
    fn test_lexer_colon_colon_isolated() -> Result<(), crate::error::HclError> {
        let input = "::";
        let mut lex = Lexer::new(input);
        let tok = lex
            .next()
            .unwrap_or_else(|| unreachable!("token"))
            .unwrap_or_else(|_| unreachable!("no lex error"));
        assert_eq!(tok.kind, TokenKind::ColonColon);
        assert_eq!(tok.text, "::");
        assert_eq!(tok.span.start_byte, 0);
        assert_eq!(tok.span.end_byte, 2);
        assert_eq!(tok.span.start_line, 1);
        assert_eq!(tok.span.start_col, 1);
        assert_eq!(tok.span.end_line, 1);
        assert_eq!(tok.span.end_col, 3);
        assert!(lex.next().is_none());
        Ok(())
    }

    #[test]
    fn test_lexer_colon_colon_chained_and_odd() -> Result<(), crate::error::HclError> {
        // ":::" should be ColonColon followed by Colon
        let input = ":::";
        let mut lex = Lexer::new(input);
        let t1 = lex
            .next()
            .unwrap_or_else(|| unreachable!("token 1"))
            .unwrap_or_else(|_| unreachable!("ok 1"));
        assert_eq!(t1.kind, TokenKind::ColonColon);
        assert_eq!(t1.span.start_byte, 0);
        assert_eq!(t1.span.end_byte, 2);

        let t2 = lex
            .next()
            .unwrap_or_else(|| unreachable!("token 2"))
            .unwrap_or_else(|_| unreachable!("ok 2"));
        assert_eq!(t2.kind, TokenKind::Colon);
        assert_eq!(t2.span.start_byte, 2);
        assert_eq!(t2.span.end_byte, 3);
        assert!(lex.next().is_none());

        // "::::" should be two ColonColon tokens
        let input4 = "::::";
        let mut lex4 = Lexer::new(input4);
        let t1 = lex4
            .next()
            .unwrap_or_else(|| unreachable!("token 1"))
            .unwrap_or_else(|_| unreachable!("ok 1"));
        assert_eq!(t1.kind, TokenKind::ColonColon);
        let t2 = lex4
            .next()
            .unwrap_or_else(|| unreachable!("token 2"))
            .unwrap_or_else(|_| unreachable!("ok 2"));
        assert_eq!(t2.kind, TokenKind::ColonColon);
        assert!(lex4.next().is_none());
        Ok(())
    }

    #[test]
    fn test_lexer_namespaced_identifier_stream() -> Result<(), crate::error::HclError> {
        let input = "aws::s3::bucket_name";
        let mut lex = Lexer::new(input);

        let t1 = lex
            .next()
            .unwrap_or_else(|| unreachable!("aws"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(t1.kind, TokenKind::Ident);
        assert_eq!(t1.text, "aws");
        assert_eq!(t1.span.start_byte, 0);
        assert_eq!(t1.span.end_byte, 3);

        let t2 = lex
            .next()
            .unwrap_or_else(|| unreachable!("::"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(t2.kind, TokenKind::ColonColon);
        assert_eq!(t2.span.start_byte, 3);
        assert_eq!(t2.span.end_byte, 5);

        let t3 = lex
            .next()
            .unwrap_or_else(|| unreachable!("s3"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(t3.kind, TokenKind::Ident);
        assert_eq!(t3.text, "s3");
        assert_eq!(t3.span.start_byte, 5);
        assert_eq!(t3.span.end_byte, 7);

        let t4 = lex
            .next()
            .unwrap_or_else(|| unreachable!("::"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(t4.kind, TokenKind::ColonColon);
        assert_eq!(t4.span.start_byte, 7);
        assert_eq!(t4.span.end_byte, 9);

        let t5 = lex
            .next()
            .unwrap_or_else(|| unreachable!("bucket_name"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(t5.kind, TokenKind::Ident);
        assert_eq!(t5.text, "bucket_name");
        assert_eq!(t5.span.start_byte, 9);
        assert_eq!(t5.span.end_byte, 20);

        assert!(lex.next().is_none());
        Ok(())
    }

    #[test]
    fn test_quoted_string_edge_cases() -> Result<(), crate::error::HclError> {
        let mut lex_empty = Lexer::new("");
        assert!(lex_empty.parse_quoted_string().is_none());

        let mut lex_non_quote = Lexer::new("not_a_quote");
        assert!(lex_non_quote.parse_quoted_string().is_none());

        // Trailing backslash in outer string
        let mut lex_trailing_slash = Lexer::new("\"hello\\");
        assert!(lex_trailing_slash.parse_quoted_string().is_none());

        // Escaped quotes inside interpolation
        let input_escaped = "\"prefix-${ \"hello \\\" world\" }-suffix\"";
        let mut lex_escaped = Lexer::new(input_escaped);
        let tok = lex_escaped
            .next()
            .unwrap_or_else(|| unreachable!("some"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(tok.kind, TokenKind::String);
        assert_eq!(tok.text, input_escaped);

        // Trailing backslash inside inner string
        let mut lex_inner_slash = Lexer::new("\"${ \"unterminated\\");
        assert!(lex_inner_slash.parse_quoted_string().is_none());

        // Unterminated inner string without trailing backslash
        let mut lex_inner_plain = Lexer::new("\"${ \"unterminated string");
        assert!(lex_inner_plain.parse_quoted_string().is_none());

        // Escaped $${ and %%{
        let input_escaped_interp = "\"prefix $${escaped} and %%{directive}\"";
        let mut lex_interp = Lexer::new(input_escaped_interp);
        let tok_interp = lex_interp
            .next()
            .unwrap_or_else(|| unreachable!("some"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(tok_interp.kind, TokenKind::String);

        // Escaped with backslash before $$
        let input_bs_dollar = "\"bs \\$${not_escaped}\"";
        let mut lex_bs = Lexer::new(input_bs_dollar);
        let tok_bs = lex_bs
            .next()
            .unwrap_or_else(|| unreachable!("some"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(tok_bs.kind, TokenKind::String);

        // Directive %{
        let input_directive = "\"%{if true}content%{endif}\"";
        let mut lex_dir = Lexer::new(input_directive);
        let tok_dir = lex_dir
            .next()
            .unwrap_or_else(|| unreachable!("some"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(tok_dir.kind, TokenKind::String);

        // Unclosed interpolation
        let mut lex_unclosed_interp = Lexer::new("\"${unclosed");
        assert!(lex_unclosed_interp.parse_quoted_string().is_none());

        // Literal dollar and percent not followed by '{'
        let input_dollar = "\"cost is $100 and 50% off or trailing $\"";
        let mut lex_dollar = Lexer::new(input_dollar);
        let tok_dollar = lex_dollar
            .next()
            .unwrap_or_else(|| unreachable!("some"))
            .unwrap_or_else(|_| unreachable!("ok"));
        assert_eq!(tok_dollar.kind, TokenKind::String);
        assert_eq!(tok_dollar.text, input_dollar);

        // Unclosed string ending in literal $ or % at EOF (idx + 1 < bytes.len() is false)
        let mut lex_unclosed_dollar = Lexer::new("\"unclosed $");
        assert!(lex_unclosed_dollar.parse_quoted_string().is_none());

        let mut lex_unclosed_percent = Lexer::new("\"unclosed %");
        assert!(lex_unclosed_percent.parse_quoted_string().is_none());
        Ok(())
    }
}
