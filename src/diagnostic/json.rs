//! Standard machine-readable diagnostics JSON serialization.
//!
//! Provides JSON data structures matching `HashiCorp`'s official diagnostic JSON specification:
//!
//! ```json
//! {
//!   "severity": "error",
//!   "summary": "...",
//!   "detail": "...",
//!   "address": "...",
//!   "range": { "filename": "...", "start": { "line": 1, "column": 1, "byte": 0 }, "end": { "line": 1, "column": 10, "byte": 9 } },
//!   "snippet": { "context": "...", "code": "...", "start_line": 1 }
//! }
//! ```

use crate::diagnostic::{Diagnostic, Diagnostics, Severity};
use crate::error::HclError;
use crate::span::Span;
use serde::{Deserialize, Serialize};

/// Position within a source file for JSON diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPosJson {
    /// 1-indexed line number.
    pub line: usize,
    /// 1-indexed column number.
    pub column: usize,
    /// 0-indexed byte offset.
    pub byte: usize,
}

/// Source range within a file for JSON diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRangeJson {
    /// File name or path.
    pub filename: String,
    /// Starting position.
    pub start: DiagnosticPosJson,
    /// Ending position.
    pub end: DiagnosticPosJson,
}

/// Code snippet excerpt for JSON diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticSnippetJson {
    /// Optional surrounding context description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Source code line(s) around the issue.
    pub code: String,
    /// 1-indexed starting line number of the code excerpt.
    pub start_line: usize,
}

/// HashiCorp-compatible JSON representation of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticJson {
    /// Severity level (`"error"` or `"warning"`).
    pub severity: String,
    /// Short summary of the diagnostic.
    pub summary: String,
    /// Detailed diagnostic guidance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Target configuration address or attribute path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Source code range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<DiagnosticRangeJson>,
    /// Source code excerpt snippet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<DiagnosticSnippetJson>,
}

impl DiagnosticJson {
    /// Constructs a `DiagnosticJson` from a [`Diagnostic`] and optional source text.
    ///
    /// # Arguments
    /// * `diag` - The diagnostic to serialize.
    /// * `source` - Optional source file content used to extract code snippets.
    ///
    /// # Returns
    /// The serialized JSON representation of this diagnostic.
    #[must_use]
    pub fn from_diagnostic(diag: &Diagnostic, source: Option<&str>) -> Self {
        let severity = match diag.severity {
            Severity::Error => "error".to_string(),
            Severity::Warning => "warning".to_string(),
        };

        let summary = diag
            .summary
            .clone()
            .unwrap_or_else(|| diag.error.to_string());

        let filename = diag
            .subject
            .file
            .as_deref()
            .map_or_else(String::new, ToString::to_string);

        let range = if diag.subject.start_line > 0 || diag.subject.end_byte > 0 {
            Some(DiagnosticRangeJson {
                filename,
                start: DiagnosticPosJson {
                    line: diag.subject.start_line,
                    column: diag.subject.start_col,
                    byte: diag.subject.start_byte,
                },
                end: DiagnosticPosJson {
                    line: diag.subject.end_line,
                    column: diag.subject.end_col,
                    byte: diag.subject.end_byte,
                },
            })
        } else {
            None
        };

        let snippet = source.and_then(|s| extract_snippet(&diag.subject, s));

        Self {
            severity,
            summary,
            detail: diag.detail.clone(),
            address: diag.address.clone(),
            range,
            snippet,
        }
    }

    /// Converts this `DiagnosticJson` back into a strongly-typed [`Diagnostic`].
    ///
    /// # Returns
    /// The reconstructed [`Diagnostic`].
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic {
        let severity = if self.severity == "warning" {
            Severity::Warning
        } else {
            Severity::Error
        };

        let subject = if let Some(ref r) = self.range {
            let file = if r.filename.is_empty() {
                None
            } else {
                Some(std::sync::Arc::from(r.filename.as_str()))
            };
            Span::new_with_file(
                r.start.byte,
                r.end.byte,
                r.start.line,
                r.start.column,
                r.end.line,
                r.end.column,
                file,
            )
        } else {
            Span::default()
        };

        let error = HclError::Parse(self.summary.clone());
        let mut diag = Diagnostic::new(error, subject);
        diag.severity = severity;
        diag.summary = Some(self.summary.clone());
        diag.detail.clone_from(&self.detail);
        diag.address.clone_from(&self.address);
        diag
    }
}

fn extract_snippet(span: &Span, source: &str) -> Option<DiagnosticSnippetJson> {
    if source.is_empty() || span.start_line == 0 {
        return None;
    }
    let lines: Vec<&str> = source.lines().collect();
    let start_idx = span.start_line.saturating_sub(1);
    if start_idx >= lines.len() {
        return None;
    }
    let end_idx = span
        .end_line
        .saturating_sub(1)
        .max(start_idx)
        .min(lines.len() - 1);
    let code = lines[start_idx..=end_idx].join("\n");
    Some(DiagnosticSnippetJson {
        context: None,
        code,
        start_line: span.start_line,
    })
}

/// Serializes a collection of diagnostics to a JSON string.
///
/// # Arguments
/// * `diags` - The diagnostics to serialize.
/// * `source` - Optional source file content for code snippets.
///
/// # Errors
/// Returns [`HclError::CtyJson`] if JSON serialization fails.
pub fn diagnostics_to_json(diags: &Diagnostics, source: Option<&str>) -> Result<String, HclError> {
    let list: Vec<DiagnosticJson> = diags
        .iter()
        .map(|d| DiagnosticJson::from_diagnostic(d, source))
        .collect();
    diagnostics_list_to_json(&list)
}

fn diagnostics_list_to_json<T: serde::Serialize>(val: &T) -> Result<String, HclError> {
    serde_json::to_string_pretty(val).map_err(|e| HclError::CtyJson(e.to_string()))
}

pub(crate) fn diagnostic_to_json<T: serde::Serialize>(val: &T) -> Result<String, HclError> {
    serde_json::to_string(val).map_err(|e| HclError::CtyJson(e.to_string()))
}

/// Deserializes a collection of diagnostics from a JSON string.
///
/// # Arguments
/// * `json_str` - The JSON string to parse.
///
/// # Errors
/// Returns [`HclError::CtyJson`] if parsing fails.
pub fn diagnostics_from_json(json_str: &str) -> Result<Diagnostics, HclError> {
    let list: Vec<DiagnosticJson> =
        serde_json::from_str(json_str).map_err(|e| HclError::CtyJson(e.to_string()))?;
    let mut diags = Diagnostics::new();
    for item in list {
        diags.push(item.to_diagnostic());
    }
    Ok(diags)
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

    struct FailingSerialize;
    impl serde::Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("forced failure"))
        }
    }

    #[test]
    fn test_diagnostic_json_roundtrip() {
        let span = Span::new_with_file(0, 10, 1, 1, 1, 11, Some(std::sync::Arc::from("test.hcl")));
        let mut diag = Diagnostic::new(HclError::Parse("Syntax error".to_string()), span);
        diag.summary = Some("Syntax error".to_string());
        diag.detail = Some("Unexpected token found".to_string());
        diag.address = Some("resource.aws_instance.foo".to_string());

        let source = "foo = bar
baz = 123
";
        let json_str = diagnostics_to_json(&Diagnostics::from(diag.clone()), Some(source)).unwrap();

        assert!(json_str.contains("\"severity\": \"error\""));
        assert!(json_str.contains("\"summary\": \"Syntax error\""));
        assert!(json_str.contains("\"detail\": \"Unexpected token found\""));
        assert!(json_str.contains("\"address\": \"resource.aws_instance.foo\""));
        assert!(json_str.contains("\"filename\": \"test.hcl\""));
        assert!(json_str.contains("\"code\": \"foo = bar\""));

        let deserialized = diagnostics_from_json(&json_str).unwrap();
        assert_eq!(deserialized.errors().len(), 1);
        let d = &deserialized.errors()[0];
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.summary, Some("Syntax error".to_string()));
        assert_eq!(d.detail, Some("Unexpected token found".to_string()));
        assert_eq!(d.address, Some("resource.aws_instance.foo".to_string()));
        assert_eq!(d.subject.start_line, 1);
        assert_eq!(d.subject.start_col, 1);
    }

    #[test]
    fn test_diagnostic_json_warning_and_no_source() {
        let span = Span::default();
        let mut diag = Diagnostic::new(HclError::Lint("Unused var".to_string()), span);
        diag.severity = Severity::Warning;
        diag.summary = Some("Unused var".to_string());

        let json_str = diagnostics_to_json(&Diagnostics::from(diag), None).unwrap();
        assert!(json_str.contains("\"severity\": \"warning\""));
        assert!(!json_str.contains("\"snippet\""));

        let deserialized = diagnostics_from_json(&json_str).unwrap();
        assert_eq!(deserialized.errors().len(), 1);
        assert_eq!(deserialized.errors()[0].severity, Severity::Warning);
    }

    #[test]
    fn test_extract_snippet_empty_or_out_of_bounds() {
        let span = Span::new(0, 5, 10, 1, 10, 6);
        let snippet = extract_snippet(
            &span,
            "line 1
line 2
",
        );
        assert!(snippet.is_none());

        let empty_span = Span::default();
        let snippet_empty = extract_snippet(&empty_span, "content");
        assert!(snippet_empty.is_none());
    }

    /// Tests fallback when summary is None, range with byte offsets, and serialization/deserialization errors.
    #[test]
    fn test_diagnostic_json_fallback_and_branches() {
        let span = Span::new(0, 5, 0, 0, 0, 5);
        let mut diag = Diagnostic::new(HclError::Parse("No summary error".to_string()), span);
        diag.summary = None;

        let json = DiagnosticJson::from_diagnostic(&diag, None);
        assert_eq!(json.summary, "Parse error: No summary error");
        assert!(json.range.is_some());

        // Range without filename converts to diagnostic with default file
        let reconstructed = json.to_diagnostic();
        assert_eq!(reconstructed.subject.file, None);

        // Serialization error test
        let err = diagnostics_list_to_json(&FailingSerialize);
        assert!(err.is_err());
        assert!(diagnostic_to_json(&FailingSerialize).is_err());

        // Deserialization error test
        let parse_err = diagnostics_from_json("not valid json");
        assert!(parse_err.is_err());

        // Snippet extraction with empty source
        let empty_source_snippet = extract_snippet(&Span::new(0, 5, 1, 1, 1, 6), "");
        assert!(empty_source_snippet.is_none());
    }
}
