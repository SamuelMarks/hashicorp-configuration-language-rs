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

            // Advance the underlying lexer by this exact byte count.
            let span_start = self.inner.span().start; // Get current absolute start BEFORE bump
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
}

impl Iterator for Lexer<'_> {
    type Item = Result<Token, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Intercept Heredoc `<<` parsing before standard lexer handles `<`
        let remainder = self.inner.remainder();
        if remainder.starts_with("<<")
            && let Some(heredoc_token) = self.parse_heredoc()
        {
            return Some(Ok(heredoc_token));
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
    use super::*;

    #[test]
    fn test_lexer() {
        let input = "var = 123\n# comment\nfoo";
        let mut lex = Lexer::new(input);

        let t1 = lex.next().expect("expected value").expect("expected value");
        assert_eq!(t1.kind, TokenKind::Ident);
        assert!(t1.text.contains("var"));
        assert_eq!(t1.span.start_line, 1);
        assert_eq!(t1.span.start_col, 1);

        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Whitespace
        );

        let t2 = lex.next().expect("expected value").expect("expected value");
        assert_eq!(t2.kind, TokenKind::Assign);
        assert_eq!(t2.span.start_line, 1);
        assert_eq!(t2.span.start_col, 5);

        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Whitespace
        );

        let t3 = lex.next().expect("expected value").expect("expected value");
        assert_eq!(t3.kind, TokenKind::Number);
        assert!(t3.text.contains("123"));

        let tnl1 = lex.next().expect("expected value").expect("expected value");
        assert_eq!(tnl1.kind, TokenKind::Newline);

        let tc = lex.next().expect("expected value").expect("expected value");
        assert_eq!(tc.kind, TokenKind::InlineComment);

        let tnl2 = lex.next().expect("expected value").expect("expected value");
        assert_eq!(tnl2.kind, TokenKind::Newline); // From after the comment

        let t4 = lex.next().expect("expected value").expect("expected value");
        assert_eq!(t4.kind, TokenKind::Ident);
        assert!(t4.text.contains("foo"));
        assert_eq!(t4.span.start_line, 3);
        assert_eq!(t4.span.start_col, 1);

        assert!(lex.next().is_none());
    }

    #[test]
    fn test_lexer_line_col_exact() {
        let input = "a\nb";
        let mut lex = Lexer::new(input);

        let t1 = lex.next().expect("expected value").expect("expected value");
        assert_eq!(t1.span.start_line, 1);
        assert_eq!(t1.span.start_col, 1);
        assert_eq!(t1.span.end_line, 1);
        assert_eq!(t1.span.end_col, 2);

        let tnl = lex.next().expect("expected value").expect("expected value");
        assert_eq!(tnl.kind, TokenKind::Newline);

        let t2 = lex.next().expect("expected value").expect("expected value");
        assert_eq!(t2.span.start_line, 2);
        assert_eq!(t2.span.start_col, 1);
        assert_eq!(t2.span.end_line, 2);
        assert_eq!(t2.span.end_col, 2);
    }

    #[test]
    fn test_lexer_error() {
        // an invalid token that doesn't match any rule (e.g. backtick which is not in HCL)
        let input = "`";
        let mut lex = Lexer::new(input);
        let res = lex.next().expect("some token");
        assert!(res.is_err());
    }

    #[test]
    fn test_heredoc_standard() {
        // We added a newline before the heredoc marker to make sure it matches properly on standard
        // trailing whitespace behavior.
        let input = "a = <<EOF\nhello\nEOF\nb = 2";
        let mut lex = Lexer::new(input);

        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Ident
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Whitespace
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Assign
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Whitespace
        );

        let hd = lex.next().expect("expected value").expect("expected value");
        assert_eq!(hd.kind, TokenKind::Heredoc);
        assert!(hd.text.contains("<<EOF\nhello\nEOF"));

        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Newline
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Ident
        );
    }

    #[test]
    fn test_heredoc_indented() {
        let input = "<<-MARKER\n  indented\n  MARKER\n";
        let mut lex = Lexer::new(input);

        let hd = lex.next().expect("expected value").expect("expected value");
        assert_eq!(hd.kind, TokenKind::Heredoc);
        assert!(hd.text.contains("<<-MARKER\n  indented\n  MARKER"));
    }

    #[test]
    fn test_heredoc_invalid_marker() {
        // If it starts with << but has no marker or doesn't end, it falls back to normal lexing
        // which will emit `<` and `<`
        let input = "<<\n";
        let mut lex = Lexer::new(input);

        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Lt
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Lt
        );
    }

    #[test]
    fn test_heredoc_unclosed() {
        let input = "<<EOF\nnever closes";
        let mut lex = Lexer::new(input);

        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Lt
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Lt
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Ident
        ); // EOF
    }

    #[test]
    fn test_heredoc_no_newline() {
        // A heredoc token without a newline is not a valid heredoc, falls back to normal lexing
        let input = "<<EOF";
        let mut lex = Lexer::new(input);
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Lt
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Lt
        );
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Ident
        );
    }

    #[test]
    fn test_not_heredoc_but_starts_with_lt() {
        let input = "<";
        let mut lex = Lexer::new(input);
        assert_eq!(
            lex.next()
                .expect("expected value")
                .expect("expected value")
                .kind,
            TokenKind::Lt
        );
    }

    #[test]
    fn test_heredoc_parser_bails_if_not_starts_with_lt_lt() {
        let input = "abc";
        let mut lex = Lexer::new(input);
        assert_eq!(lex.parse_heredoc(), None);
    }

    #[test]
    fn test_lexer_colon_colon_isolated() {
        let input = "::";
        let mut lex = Lexer::new(input);
        let tok = lex.next().expect("token").expect("no lex error");
        assert_eq!(tok.kind, TokenKind::ColonColon);
        assert_eq!(tok.text, "::");
        assert_eq!(tok.span.start_byte, 0);
        assert_eq!(tok.span.end_byte, 2);
        assert_eq!(tok.span.start_line, 1);
        assert_eq!(tok.span.start_col, 1);
        assert_eq!(tok.span.end_line, 1);
        assert_eq!(tok.span.end_col, 3);
        assert!(lex.next().is_none());
    }

    #[test]
    fn test_lexer_colon_colon_chained_and_odd() {
        // ":::" should be ColonColon followed by Colon
        let input = ":::";
        let mut lex = Lexer::new(input);
        let t1 = lex.next().expect("token 1").expect("ok 1");
        assert_eq!(t1.kind, TokenKind::ColonColon);
        assert_eq!(t1.span.start_byte, 0);
        assert_eq!(t1.span.end_byte, 2);

        let t2 = lex.next().expect("token 2").expect("ok 2");
        assert_eq!(t2.kind, TokenKind::Colon);
        assert_eq!(t2.span.start_byte, 2);
        assert_eq!(t2.span.end_byte, 3);
        assert!(lex.next().is_none());

        // "::::" should be two ColonColon tokens
        let input4 = "::::";
        let mut lex4 = Lexer::new(input4);
        let t1 = lex4.next().expect("token 1").expect("ok 1");
        assert_eq!(t1.kind, TokenKind::ColonColon);
        let t2 = lex4.next().expect("token 2").expect("ok 2");
        assert_eq!(t2.kind, TokenKind::ColonColon);
        assert!(lex4.next().is_none());
    }

    #[test]
    fn test_lexer_namespaced_identifier_stream() {
        let input = "aws::s3::bucket_name";
        let mut lex = Lexer::new(input);

        let t1 = lex.next().expect("aws").expect("ok");
        assert_eq!(t1.kind, TokenKind::Ident);
        assert_eq!(t1.text, "aws");
        assert_eq!(t1.span.start_byte, 0);
        assert_eq!(t1.span.end_byte, 3);

        let t2 = lex.next().expect("::").expect("ok");
        assert_eq!(t2.kind, TokenKind::ColonColon);
        assert_eq!(t2.span.start_byte, 3);
        assert_eq!(t2.span.end_byte, 5);

        let t3 = lex.next().expect("s3").expect("ok");
        assert_eq!(t3.kind, TokenKind::Ident);
        assert_eq!(t3.text, "s3");
        assert_eq!(t3.span.start_byte, 5);
        assert_eq!(t3.span.end_byte, 7);

        let t4 = lex.next().expect("::").expect("ok");
        assert_eq!(t4.kind, TokenKind::ColonColon);
        assert_eq!(t4.span.start_byte, 7);
        assert_eq!(t4.span.end_byte, 9);

        let t5 = lex.next().expect("bucket_name").expect("ok");
        assert_eq!(t5.kind, TokenKind::Ident);
        assert_eq!(t5.text, "bucket_name");
        assert_eq!(t5.span.start_byte, 9);
        assert_eq!(t5.span.end_byte, 20);

        assert!(lex.next().is_none());
    }
}
