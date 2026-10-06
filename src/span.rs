//! Span representation.
pub mod spatial;
pub use spatial::{AstNodeRef, Position};
use std::sync::Arc;
/// Represents a specific region of source code.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// The optional source file identifier or path.
    pub file: Option<Arc<str>>,
    /// The starting byte offset.
    pub start_byte: usize,
    /// The ending byte offset (exclusive).
    pub end_byte: usize,
    /// The starting line (1-indexed).
    pub start_line: usize,
    /// The starting column (1-indexed).
    pub start_col: usize,
    /// The ending line (1-indexed).
    pub end_line: usize,
    /// The ending column (1-indexed).
    pub end_col: usize,
}
impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref file) = self.file {
            write!(f, "{file}:{}:{}", self.start_line, self.start_col)
        } else {
            write!(f, "{}:{}", self.start_line, self.start_col)
        }
    }
}
impl Span {
    /// Merges two spans into one covering both, preserving common file identity.
    #[must_use]
    pub fn merge(&self, other: &Span) -> Span {
        let file = match (&self.file, &other.file) {
            (Some(f1), Some(f2)) if f1 == f2 => Some(f1.clone()),
            (Some(f1), None) => Some(f1.clone()),
            (None, Some(f2)) => Some(f2.clone()),
            _ => None,
        };
        let start_byte = self.start_byte.min(other.start_byte);
        let end_byte = self.end_byte.max(other.end_byte);
        let (start_line, start_col) = if self.start_byte < other.start_byte {
            (self.start_line, self.start_col)
        } else {
            (other.start_line, other.start_col)
        };
        let (end_line, end_col) = if self.end_byte > other.end_byte {
            (self.end_line, self.end_col)
        } else {
            (other.end_line, other.end_col)
        };
        Span::new_with_file(
            start_byte, end_byte, start_line, start_col, end_line, end_col, file,
        )
    }
    /// Create a new `Span` without a file identifier.
    #[must_use]
    pub fn new(
        start_byte: usize,
        end_byte: usize,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> Self {
        Self {
            file: None,
            start_byte,
            end_byte,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
    /// Create a new `Span` with an optional source file identifier.
    ///
    /// # Arguments
    /// * `start_byte` - Starting byte offset.
    /// * `end_byte` - Ending byte offset.
    /// * `start_line` - Starting line number (1-indexed).
    /// * `start_col` - Starting column number (1-indexed).
    /// * `end_line` - Ending line number (1-indexed).
    /// * `end_col` - Ending column number (1-indexed).
    /// * `file` - Optional source file path or identifier.
    #[must_use]
    pub fn new_with_file(
        start_byte: usize,
        end_byte: usize,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
        file: Option<Arc<str>>,
    ) -> Self {
        Self {
            file,
            start_byte,
            end_byte,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
    /// Returns a copy of this span associated with the specified source file.
    ///
    /// # Arguments
    /// * `file` - Source file path or identifier.
    #[must_use]
    pub fn with_file(mut self, file: impl Into<Arc<str>>) -> Self {
        self.file = Some(file.into());
        self
    }
    /// Checks whether this span contains the given spatial [`Position`].
    ///
    /// Evaluates byte offsets if available (`end_byte > start_byte`), and verifies
    /// line and column ranges. Returns `true` if `pos` lies within this span's bounds.
    ///
    /// # Arguments
    /// * `pos` - The coordinate position to check.
    #[must_use]
    pub fn contains_position(&self, pos: &Position) -> bool {
        if self.end_byte > self.start_byte {
            return pos.byte_offset >= self.start_byte && pos.byte_offset < self.end_byte;
        }
        if self.start_byte == self.end_byte && self.start_byte > 0 {
            return pos.byte_offset == self.start_byte;
        }
        if pos.line < self.start_line || pos.line > self.end_line {
            return false;
        }
        if self.start_line == self.end_line {
            return pos.column >= self.start_col && pos.column < self.end_col;
        }
        if pos.line == self.start_line {
            return pos.column >= self.start_col;
        }
        if pos.line == self.end_line {
            return pos.column < self.end_col;
        }
        true
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
    #[test]
    fn test_span_new() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        assert_eq!(span.start_byte, 0);
        assert_eq!(span.end_byte, 10);
        assert_eq!(span.start_line, 1);
        assert_eq!(span.start_col, 1);
        assert_eq!(span.end_line, 1);
        assert_eq!(span.end_col, 11);
    }
    #[test]
    fn test_span_merge() {
        let span1 = Span::new(0, 5, 1, 1, 1, 6);
        let span2 = Span::new(6, 10, 1, 7, 1, 11);
        let merged1 = span1.merge(&span2);
        assert_eq!(merged1.start_byte, 0);
        assert_eq!(merged1.end_byte, 10);
        assert_eq!(merged1.start_line, 1);
        assert_eq!(merged1.start_col, 1);
        assert_eq!(merged1.end_line, 1);
        assert_eq!(merged1.end_col, 11);
        let merged2 = span2.merge(&span1);
        assert_eq!(merged2.start_byte, 0);
        assert_eq!(merged2.end_byte, 10);
        assert_eq!(merged2.start_line, 1);
        assert_eq!(merged2.start_col, 1);
        assert_eq!(merged2.end_line, 1);
        assert_eq!(merged2.end_col, 11);
        let span3 = Span::new(0, 10, 1, 1, 1, 11);
        let merged3 = span1.merge(&span3);
        assert_eq!(merged3.start_byte, 0);
        assert_eq!(merged3.end_byte, 10);
        assert_eq!(merged3.start_line, 1);
        assert_eq!(merged3.start_col, 1);
        assert_eq!(merged3.end_line, 1);
        assert_eq!(merged3.end_col, 11);
        let file_a: Option<Arc<str>> = Some(Arc::from("main.tf"));
        let file_b: Option<Arc<str>> = Some(Arc::from("module.tf"));
        let span_a = Span::new_with_file(0, 5, 1, 1, 1, 6, file_a.clone());
        let span_a2 = Span::new_with_file(6, 10, 1, 7, 1, 11, file_a.clone());
        let span_b = Span::new_with_file(0, 10, 1, 1, 1, 11, file_b);
        let merged_same = span_a.merge(&span_a2);
        assert_eq!(merged_same.file, file_a);
        assert_eq!(merged_same.to_string(), "main.tf:1:1");
        let merged_diff = span_a.merge(&span_b);
        assert_eq!(merged_diff.file, None);
        assert_eq!(merged_diff.to_string(), "1:1");
        let span_none = Span::new(0, 5, 1, 1, 1, 6);
        let merged_with_none = span_a.merge(&span_none);
        assert_eq!(merged_with_none.file, file_a);
        let merged_none_first = span_none.merge(&span_a);
        assert_eq!(merged_none_first.file, file_a);
    }
    #[test]
    fn test_span_contains_position() {
        let mut point_span = Span::new(10, 10, 2, 5, 2, 5);
        let pos_match = Position::new(2, 5, 10);
        let pos_mismatch = Position::new(2, 6, 11);
        assert!(point_span.contains_position(&pos_match));
        assert!(!point_span.contains_position(&pos_mismatch));
        point_span.start_byte = 0;
        point_span.end_byte = 0;
        point_span.start_line = 3;
        point_span.end_line = 5;
        point_span.start_col = 10;
        point_span.end_col = 20;
        assert!(!point_span.contains_position(&Position::new(2, 10, 0)));
        assert!(!point_span.contains_position(&Position::new(6, 10, 0)));
        assert!(!point_span.contains_position(&Position::new(3, 9, 0)));
        assert!(point_span.contains_position(&Position::new(3, 10, 0)));
        assert!(point_span.contains_position(&Position::new(3, 15, 0)));
        assert!(point_span.contains_position(&Position::new(4, 1, 0)));
        assert!(point_span.contains_position(&Position::new(5, 19, 0)));
        assert!(!point_span.contains_position(&Position::new(5, 20, 0)));
        let single_line_span = Span {
            start_byte: 0,
            end_byte: 0,
            start_line: 4,
            end_line: 4,
            start_col: 5,
            end_col: 15,
            file: None,
        };
        assert!(!single_line_span.contains_position(&Position::new(4, 4, 0)));
        assert!(single_line_span.contains_position(&Position::new(4, 5, 0)));
        assert!(single_line_span.contains_position(&Position::new(4, 14, 0)));
        assert!(!single_line_span.contains_position(&Position::new(4, 15, 0)));
        let inverted_span = Span {
            start_byte: 10,
            end_byte: 5,
            start_line: 4,
            end_line: 4,
            start_col: 5,
            end_col: 15,
            file: None,
        };
        assert!(inverted_span.contains_position(&Position::new(4, 6, 0)));
    }
}
