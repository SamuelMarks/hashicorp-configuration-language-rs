//! Diagnostics and error handling.

pub mod html;
pub mod json;
pub mod suggestion;

pub use html::{HtmlDiagnosticWriter, diagnostics_to_html};
pub use json::{
    DiagnosticJson, DiagnosticPosJson, DiagnosticRangeJson, DiagnosticSnippetJson,
    diagnostics_from_json, diagnostics_to_json,
};

use crate::error::HclError;
use crate::span::Span;
use std::collections::BTreeSet;
use std::fmt::Write as FmtWrite;

/// The severity of a diagnostic message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// An error that prevents successful parsing or evaluation.
    Error,
    /// A warning that does not prevent success but should be noted.
    Warning,
}

/// A runtime sub-expression evaluation snapshot attached to a diagnostic.
///
/// Captures intermediate evaluated values of expressions participating in an error,
/// analogous to `HashiCorp` HCL's `EvalContext` diagnostic callouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalCallout {
    /// Exact source span of the sub-expression.
    pub span: Span,
    /// Textual representation of the sub-expression (e.g. `"var.foo.bar"`).
    pub expression_text: String,
    /// Evaluated string value (automatically masked to `"(sensitive value)"` if sensitive).
    pub evaluated_value: String,
}

impl EvalCallout {
    /// Creates a new sub-expression evaluation callout from a [`Value`](crate::types::Value).
    ///
    /// Automatically replaces values marked sensitive with `"(sensitive value)"`.
    ///
    /// # Arguments
    /// * `span` - The source span of the sub-expression.
    /// * `expression_text` - The expression text.
    /// * `val` - The evaluated value.
    ///
    /// # Examples
    /// ```rust
    /// use hashicorp_configuration_language_rs::diagnostic::EvalCallout;
    /// use hashicorp_configuration_language_rs::span::Span;
    /// use hashicorp_configuration_language_rs::types::{Type, Value, ValueData};
    ///
    /// let val = Value::new(Type::String, ValueData::String("hello".into()));
    /// let callout = EvalCallout::from_value(Span::new(0, 5, 1, 1, 1, 6), "var.x", &val);
    /// assert_eq!(callout.expression_text, "var.x");
    /// assert_eq!(callout.evaluated_value, "\"hello\"");
    /// ```
    #[must_use]
    pub fn from_value(
        span: Span,
        expression_text: impl Into<String>,
        val: &crate::types::Value,
    ) -> Self {
        let evaluated_value = if val.is_sensitive() {
            "(sensitive value)".to_string()
        } else {
            val.to_string()
        };
        Self {
            span,
            expression_text: expression_text.into(),
            evaluated_value,
        }
    }

    /// Creates a new sub-expression evaluation callout with an explicit value string.
    ///
    /// # Arguments
    /// * `span` - Source span of the sub-expression.
    /// * `expression_text` - The expression text.
    /// * `evaluated_value` - The formatted evaluated value string.
    #[must_use]
    pub fn new(
        span: Span,
        expression_text: impl Into<String>,
        evaluated_value: impl Into<String>,
    ) -> Self {
        Self {
            span,
            expression_text: expression_text.into(),
            evaluated_value: evaluated_value.into(),
        }
    }
}

/// A diagnostic message indicating an error or warning during parsing or evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The severity of the diagnostic.
    pub severity: Severity,
    /// The strongly-typed error that caused this diagnostic.
    pub error: HclError,
    /// The primary source span where the issue occurred.
    pub subject: Span,
    /// An optional secondary context span.
    pub context: Option<Span>,
    /// An optional short summary message.
    pub summary: Option<String>,
    /// An optional detailed guidance message.
    pub detail: Option<String>,
    /// An optional configuration address or block identifier for this diagnostic.
    pub address: Option<String>,
    /// An optional structured traversal path where the diagnostic occurred.
    pub path: Option<crate::types::path::Path>,
    /// Runtime sub-expression evaluation callouts leading up to the error.
    pub eval_callouts: Vec<EvalCallout>,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for Diagnostic {}

impl Diagnostic {
    /// Create a new error diagnostic.
    #[must_use]
    pub fn new(error: HclError, subject: Span) -> Self {
        Self {
            severity: Severity::Error,
            error,
            subject,
            context: None,
            summary: None,
            detail: None,
            address: None,
            path: None,
            eval_callouts: Vec::new(),
        }
    }

    /// Create a new error diagnostic with summary, detail, and subject span.
    #[must_use]
    pub fn error(summary: impl Into<String>, detail: impl Into<String>, subject: Span) -> Self {
        let sum_str = summary.into();
        let det_str = detail.into();
        let combined = if det_str.is_empty() {
            sum_str.clone()
        } else {
            format!("{sum_str}: {det_str}")
        };
        Self {
            severity: Severity::Error,
            error: HclError::Parse(combined),
            subject,
            context: None,
            summary: Some(sum_str),
            detail: if det_str.is_empty() {
                None
            } else {
                Some(det_str)
            },
            address: None,
            path: None,
            eval_callouts: Vec::new(),
        }
    }

    /// Create a new warning diagnostic with summary, detail, and subject span.
    #[must_use]
    pub fn warning(summary: impl Into<String>, detail: impl Into<String>, subject: Span) -> Self {
        let sum_str = summary.into();
        let det_str = detail.into();
        let combined = if det_str.is_empty() {
            sum_str.clone()
        } else {
            format!("{sum_str}: {det_str}")
        };
        Self {
            severity: Severity::Warning,
            error: HclError::Parse(combined),
            subject,
            context: None,
            summary: Some(sum_str),
            detail: if det_str.is_empty() {
                None
            } else {
                Some(det_str)
            },
            address: None,
            path: None,
            eval_callouts: Vec::new(),
        }
    }

    /// Attaches a runtime sub-expression evaluation callout snapshot to this diagnostic.
    ///
    /// # Arguments
    /// * `callout` - The sub-expression callout.
    #[must_use]
    pub fn with_callout(mut self, callout: EvalCallout) -> Self {
        self.eval_callouts.push(callout);
        self
    }

    /// Attaches multiple runtime sub-expression evaluation callouts to this diagnostic.
    ///
    /// # Arguments
    /// * `callouts` - Sequence of sub-expression callouts.
    #[must_use]
    pub fn with_callouts(mut self, callouts: impl IntoIterator<Item = EvalCallout>) -> Self {
        self.eval_callouts.extend(callouts);
        self
    }

    /// Attaches a structured traversal path to this diagnostic.
    ///
    /// # Arguments
    /// * `path` - The traversal path.
    #[must_use]
    pub fn with_path(mut self, path: crate::types::path::Path) -> Self {
        self.path = Some(path);
        self
    }

    /// Set the secondary context span on this diagnostic.
    #[must_use]
    pub fn with_context(mut self, context: Span) -> Self {
        self.context = Some(context);
        self
    }

    /// Set the detailed guidance message on this diagnostic.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Set the summary message on this diagnostic.
    #[must_use]
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Set the severity on this diagnostic.
    #[must_use]
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Set the underlying typed error on this diagnostic.
    #[must_use]
    pub fn with_error(mut self, error: HclError) -> Self {
        self.error = error;
        self
    }

    /// Set an address for this diagnostic.
    ///
    /// # Arguments
    /// * `address` - Target configuration address or attribute path.
    #[must_use]
    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }

    /// Converts this diagnostic to its standard JSON value.
    ///
    /// # Arguments
    /// * `source` - Optional source text for code snippets.
    #[must_use]
    pub fn to_json_value(&self, source: Option<&str>) -> serde_json::Value {
        let dj = json::DiagnosticJson::from_diagnostic(self, source);
        serde_json::to_value(&dj).unwrap_or(serde_json::Value::Null)
    }

    /// Serializes this diagnostic into a standard JSON string.
    ///
    /// # Arguments
    /// * `source` - Optional source text for code snippets.
    ///
    /// # Errors
    /// Returns [`HclError::CtyJson`] if serialization fails.
    pub fn to_json(&self, source: Option<&str>) -> Result<String, HclError> {
        let dj = json::DiagnosticJson::from_diagnostic(self, source);
        json::diagnostic_to_json(&dj)
    }

    /// Deserializes a `Diagnostic` from a JSON string.
    ///
    /// # Arguments
    /// * `json_str` - The JSON string representing the diagnostic.
    ///
    /// # Errors
    /// Returns [`HclError::CtyJson`] if deserialization fails.
    pub fn from_json(json_str: &str) -> Result<Self, HclError> {
        let dj: json::DiagnosticJson =
            serde_json::from_str(json_str).map_err(|e| HclError::CtyJson(e.to_string()))?;
        Ok(dj.to_diagnostic())
    }

    /// Returns the summary string for this diagnostic.
    #[must_use]
    pub fn summary_str(&self) -> String {
        if let Some(ref s) = self.summary {
            s.clone()
        } else {
            self.error.to_string()
        }
    }
}

/// A collection of diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics(Vec<Diagnostic>);

impl Diagnostics {
    /// Create a new, empty collection of diagnostics.
    #[must_use]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Add a diagnostic to the collection.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.0.push(diagnostic);
    }

    /// Check if the collection contains any error-level diagnostics.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|d| d.severity == Severity::Error)
    }

    /// Return a slice of all diagnostics.
    #[must_use]
    pub fn errors(&self) -> &[Diagnostic] {
        &self.0
    }

    /// Returns an iterator over the diagnostics.
    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.0.iter()
    }

    /// Converts all diagnostics to an array of JSON objects.
    ///
    /// # Arguments
    /// * `source` - Optional source text for code snippets.
    #[must_use]
    pub fn to_json_value(&self, source: Option<&str>) -> serde_json::Value {
        let arr: Vec<serde_json::Value> = self.0.iter().map(|d| d.to_json_value(source)).collect();
        serde_json::Value::Array(arr)
    }

    /// Serializes all diagnostics into a JSON string.
    ///
    /// # Arguments
    /// * `source` - Optional source text for code snippets.
    ///
    /// # Errors
    /// Returns [`HclError::CtyJson`] if serialization fails.
    pub fn to_json(&self, source: Option<&str>) -> Result<String, HclError> {
        json::diagnostics_to_json(self, source)
    }

    /// Deserializes a collection of diagnostics from a JSON string.
    ///
    /// # Arguments
    /// * `json_str` - The JSON string to parse.
    ///
    /// # Errors
    /// Returns [`HclError::CtyJson`] if deserialization fails.
    pub fn from_json(json_str: &str) -> Result<Self, HclError> {
        json::diagnostics_from_json(json_str)
    }

    /// Converts all diagnostics into an HTML report string.
    ///
    /// # Arguments
    /// * `source` - Optional source text for code snippets.
    /// * `standalone` - `true` for a complete standalone HTML document, `false` for an embeddable snippet.
    #[must_use]
    pub fn to_html(&self, source: Option<&str>, standalone: bool) -> String {
        html::diagnostics_to_html(self, source, standalone)
    }

    /// Writes the diagnostics as an HTML report to an arbitrary output stream.
    ///
    /// # Arguments
    /// * `writer` - The output stream.
    /// * `source` - Optional source text for code snippets.
    /// * `standalone` - `true` for a complete standalone HTML document.
    ///
    /// # Errors
    /// Returns [`std::io::Error`] if writing fails.
    pub fn write_html(
        &self,
        writer: &mut dyn std::io::Write,
        source: Option<&str>,
        standalone: bool,
    ) -> std::io::Result<()> {
        html::HtmlDiagnosticWriter::new()
            .with_standalone(standalone)
            .write_html(self, source, writer)
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, d) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{d}")?;
            if let Some(ref detail) = d.detail {
                write!(f, ": {detail}")?;
            }
        }
        Ok(())
    }
}

impl From<Diagnostic> for Diagnostics {
    fn from(d: Diagnostic) -> Self {
        Self(vec![d])
    }
}

impl Diagnostics {
    /// Extend this collection with another.
    pub fn extend(&mut self, other: Diagnostics) {
        self.0.extend(other.0);
    }
}

/// Configurable renderer for diagnostics to terminal output or plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticWriter {
    /// Whether ANSI escape sequences are used for colored output.
    pub color: bool,
    /// Maximum number of source lines to display per excerpt.
    pub max_lines: usize,
}

impl Default for DiagnosticWriter {
    fn default() -> Self {
        Self {
            color: false,
            max_lines: 10,
        }
    }
}

impl DiagnosticWriter {
    /// Create a new `DiagnosticWriter` specifying whether ANSI color codes are enabled.
    #[must_use]
    pub fn new(color: bool) -> Self {
        Self {
            color,
            max_lines: 10,
        }
    }

    /// Create a plain-text `DiagnosticWriter` without ANSI escape codes.
    #[must_use]
    pub fn plain() -> Self {
        Self::new(false)
    }

    /// Create a colored `DiagnosticWriter` using ANSI escape codes.
    #[must_use]
    pub fn colored() -> Self {
        Self::new(true)
    }

    /// Toggle ANSI color output on this writer.
    #[must_use]
    pub fn with_color(mut self, color: bool) -> Self {
        self.color = color;
        self
    }

    /// Set the maximum number of source lines to render per diagnostic excerpt.
    #[must_use]
    pub fn with_max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = max_lines;
        self
    }

    /// Format a single diagnostic into a string.
    #[must_use]
    pub fn format_diagnostic(&self, diag: &Diagnostic, filename: &str, source: &str) -> String {
        let mut out = String::new();
        self.render_diagnostic(diag, filename, source, &mut out);
        out
    }

    /// Format a collection of diagnostics into a string, separated by newlines.
    #[must_use]
    pub fn format_diagnostics(&self, diags: &Diagnostics, filename: &str, source: &str) -> String {
        let mut out = String::new();
        for (i, diag) in diags.errors().iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            self.render_diagnostic(diag, filename, source, &mut out);
        }
        out
    }

    /// Formats a value for diagnostic presentation, masking any sensitive values with `"(sensitive value)"`.
    ///
    /// # Arguments
    /// * `val` - The value to format.
    #[must_use]
    pub fn format_value(&self, val: &crate::types::Value) -> String {
        if val.is_sensitive() {
            "(sensitive value)".to_string()
        } else {
            val.to_string()
        }
    }

    /// Formats a plan diff between two values, masking any sensitive values with `"(sensitive value)"`.
    ///
    /// # Arguments
    /// * `old_val` - The original value before mutation.
    /// * `new_val` - The new proposed value.
    #[must_use]
    pub fn format_diff(
        &self,
        old_val: &crate::types::Value,
        new_val: &crate::types::Value,
    ) -> String {
        let old_str = if old_val.has_mark(&crate::types::ValueMark::Sensitive) {
            "(sensitive value)".to_string()
        } else {
            old_val.to_string()
        };
        let new_str = if new_val.has_mark(&crate::types::ValueMark::Sensitive) {
            "(sensitive value)".to_string()
        } else {
            new_val.to_string()
        };
        if self.color {
            format!("\x1b[31m- {old_str}\x1b[0m\n\x1b[32m+ {new_str}\x1b[0m")
        } else {
            format!("- {old_str}\n+ {new_str}")
        }
    }

    /// Write a formatted diagnostic into an I/O writer.
    ///
    /// # Errors
    /// Returns an error if writing to `writer` fails.
    pub fn write_diagnostic(
        &self,
        diag: &Diagnostic,
        filename: &str,
        source: &str,
        writer: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        let text = self.format_diagnostic(diag, filename, source);
        writer.write_all(text.as_bytes())
    }

    /// Write all formatted diagnostics into an I/O writer.
    ///
    /// # Errors
    /// Returns an error if writing to `writer` fails.
    pub fn write_diagnostics(
        &self,
        diags: &Diagnostics,
        filename: &str,
        source: &str,
        writer: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        let text = self.format_diagnostics(diags, filename, source);
        writer.write_all(text.as_bytes())
    }

    fn render_diagnostic(&self, diag: &Diagnostic, filename: &str, source: &str, out: &mut String) {
        let (sev_label, title_bold) = match (self.color, diag.severity) {
            (true, Severity::Error) => ("\x1b[1;31mError:\x1b[0m", "\x1b[1m"),
            (true, Severity::Warning) => ("\x1b[1;33mWarning:\x1b[0m", "\x1b[1m"),
            (false, Severity::Error) => ("Error:", ""),
            (false, Severity::Warning) => ("Warning:", ""),
        };
        let reset = if self.color { "\x1b[0m" } else { "" };
        let summary = diag.summary_str();
        let _ = writeln!(out, "{sev_label} {title_bold}{summary}{reset}");

        let effective_filename = if !filename.is_empty() {
            filename
        } else if let Some(ref f) = diag.subject.file {
            f.as_ref()
        } else {
            ""
        };

        let start_line = diag.subject.start_line;
        let start_col = diag.subject.start_col;
        if start_line > 0 {
            if effective_filename.is_empty() {
                let _ = writeln!(out, "  on line {start_line}, col {start_col}:");
            } else {
                let _ = writeln!(
                    out,
                    "  on {effective_filename} line {start_line}, col {start_col}:"
                );
            }
        }

        if let Some(ref p) = diag.path {
            let _ = writeln!(out, "  at path: {p}");
        }

        if let Some(ref ctx) = diag.context
            && let Some(ref ctx_file) = ctx.file
            && Some(ctx_file) != diag.subject.file.as_ref()
        {
            let _ = writeln!(
                out,
                "  (context in {ctx_file} line {}, col {}):",
                ctx.start_line, ctx.start_col
            );
        }

        let lines: Vec<&str> = source.lines().collect();
        if !lines.is_empty() && start_line > 0 && start_line <= lines.len() {
            let mut line_set = BTreeSet::new();

            let subj_start = diag.subject.start_line.clamp(1, lines.len());
            let subj_end = diag.subject.end_line.clamp(subj_start, lines.len());
            for l in subj_start..=subj_end {
                line_set.insert(l);
            }

            if let Some(ctx) = diag
                .context
                .as_ref()
                .filter(|c| c.start_line > 0 && c.start_line <= lines.len())
            {
                let ctx_start = ctx.start_line.clamp(1, lines.len());
                let ctx_end = ctx.end_line.clamp(ctx_start, lines.len());
                for l in ctx_start..=ctx_end {
                    line_set.insert(l);
                }
            }

            let max_line = line_set.iter().copied().max().unwrap_or(start_line);
            let gutter_width = max_line.to_string().len().max(2);

            let displayed_lines: Vec<usize> = line_set.into_iter().take(self.max_lines).collect();
            let mut prev_line = 0;

            for &line_num in &displayed_lines {
                if prev_line > 0 && line_num > prev_line + 1 {
                    let gutter_ellipsis = if self.color {
                        format!("\x1b[36m{:>gutter_width$} |\x1b[0m ...", "")
                    } else {
                        format!("{:>gutter_width$} | ...", "")
                    };
                    let _ = writeln!(out, "{gutter_ellipsis}");
                }
                prev_line = line_num;

                let line_content = lines.get(line_num - 1).copied().unwrap_or("");
                let line_gutter = if self.color {
                    format!("\x1b[36m{line_num:>gutter_width$} |\x1b[0m {line_content}")
                } else {
                    format!("{line_num:>gutter_width$} | {line_content}")
                };
                let _ = writeln!(out, "{line_gutter}");

                let (c_start, c_end) = if let Some(ref ctx) = diag.context {
                    compute_span_cols(line_num, line_content.len(), ctx)
                } else {
                    (0, 0)
                };

                let (s_start, s_end) =
                    compute_span_cols(line_num, line_content.len(), &diag.subject);

                if c_start < c_end || s_start < s_end {
                    let needed_len = line_content.len().max(s_end).max(c_end);
                    let mut underline = vec![' '; needed_len];

                    if c_start < c_end {
                        for col in c_start..c_end {
                            underline[col - 1] = '~';
                        }
                    }

                    if s_start < s_end {
                        for col in s_start..s_end {
                            underline[col - 1] = '^';
                        }
                    }

                    while underline.last() == Some(&' ') {
                        underline.pop();
                    }

                    let styled_underline = if self.color {
                        style_underline_colored(&underline)
                    } else {
                        underline.into_iter().collect()
                    };

                    let pointer_line = if self.color {
                        format!("\x1b[36m{:>gutter_width$} |\x1b[0m {styled_underline}", "")
                    } else {
                        format!("{:>gutter_width$} | {styled_underline}", "")
                    };
                    let _ = writeln!(out, "{pointer_line}");
                }
            }

            if displayed_lines.len() < max_line {
                // Truncated because of max_lines
                if self.max_lines < lines.len() {
                    let gutter_ellipsis = if self.color {
                        format!("\x1b[36m{:>gutter_width$} |\x1b[0m ...", "")
                    } else {
                        format!("{:>gutter_width$} | ...", "")
                    };
                    let _ = writeln!(out, "{gutter_ellipsis}");
                }
            }
        }

        if !diag.eval_callouts.is_empty() {
            let gutter_width = if diag.subject.start_line > 0 {
                diag.subject.start_line.to_string().len().max(2)
            } else {
                2
            };
            for (idx, callout) in diag.eval_callouts.iter().enumerate() {
                let is_last = idx + 1 == diag.eval_callouts.len();
                let branch = if is_last { "└─" } else { "├─" };
                let expr_str = if self.color {
                    format!("\x1b[1m{}\x1b[0m", callout.expression_text)
                } else {
                    callout.expression_text.clone()
                };
                let line = if self.color {
                    format!(
                        "\x1b[36m{:>gutter_width$} |\x1b[0m   {branch} {expr_str} is {}",
                        "", callout.evaluated_value
                    )
                } else {
                    format!(
                        "{:>gutter_width$} |   {branch} {expr_str} is {}",
                        "", callout.evaluated_value
                    )
                };
                let _ = writeln!(out, "{line}");
            }
        }

        if let Some(detail) = diag.detail.as_deref().filter(|s| !s.is_empty()) {
            let _ = writeln!(out, "{detail}");
        }
    }
}

fn compute_span_cols(line_num: usize, line_len: usize, span: &Span) -> (usize, usize) {
    if line_num < span.start_line || line_num > span.end_line {
        return (0, 0);
    }
    if line_num == span.start_line && line_num == span.end_line {
        let start = span.start_col.max(1);
        let end = span.end_col.max(start + 1);
        (start, end)
    } else if line_num == span.start_line {
        let start = span.start_col.max(1);
        let end = line_len.max(start) + 1;
        (start, end)
    } else if line_num == span.end_line {
        let start = 1;
        let end = span.end_col.max(2);
        (start, end)
    } else {
        let start = 1;
        let end = line_len + 1;
        (start, end)
    }
}

fn style_underline_colored(chars: &[char]) -> String {
    let mut out = String::new();
    let mut current_style = 0; // 0: none, 1: context (~), 2: subject (^)

    for &ch in chars {
        match ch {
            '~' => {
                if current_style != 1 {
                    if current_style != 0 {
                        out.push_str("\x1b[0m");
                    }
                    out.push_str("\x1b[1;33m");
                    current_style = 1;
                }
                out.push('~');
            }
            '^' => {
                if current_style != 2 {
                    if current_style != 0 {
                        out.push_str("\x1b[0m");
                    }
                    out.push_str("\x1b[1;31m");
                    current_style = 2;
                }
                out.push('^');
            }
            _ => {
                if current_style != 0 {
                    out.push_str("\x1b[0m");
                    current_style = 0;
                }
                out.push(' ');
            }
        }
    }
    if current_style != 0 {
        out.push_str("\x1b[0m");
    }
    out
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
    fn test_diagnostic_error() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let err = HclError::Parse("msg: detail".to_string());
        let diag = Diagnostic::new(err.clone(), span.clone());
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.error, err);
        assert_eq!(diag.subject, span);
        assert_eq!(diag.context, None);
        assert_eq!(diag.summary, None);
        assert_eq!(diag.detail, None);
        assert!(diag.to_string().contains("Parse error: msg: detail"));
        assert_eq!(diag.summary_str(), "Parse error: msg: detail");
    }

    #[test]
    fn test_diagnostic_builders() {
        let span1 = Span::new(0, 5, 1, 1, 1, 6);
        let span2 = Span::new(10, 15, 2, 1, 2, 6);

        let d1 = Diagnostic::error("summary", "detail", span1.clone())
            .with_context(span2.clone())
            .with_severity(Severity::Warning)
            .with_summary("custom summary")
            .with_detail("custom detail")
            .with_error(HclError::Validation("custom error".into()));

        assert_eq!(d1.severity, Severity::Warning);
        assert_eq!(d1.context, Some(span2));
        assert_eq!(d1.summary_str(), "custom summary");
        assert_eq!(d1.detail, Some("custom detail".to_string()));
        assert_eq!(d1.error, HclError::Validation("custom error".into()));

        let d2 = Diagnostic::warning("warn sum", "warn det", span1.clone());
        assert_eq!(d2.severity, Severity::Warning);
        assert_eq!(d2.summary_str(), "warn sum");
        assert_eq!(d2.detail, Some("warn det".to_string()));

        let d3 = Diagnostic::error("simple sum", "", span1.clone());
        assert_eq!(d3.detail, None);

        let d4 = Diagnostic::warning("simple warn", "", span1);
        assert_eq!(d4.detail, None);
    }

    #[test]
    fn test_diagnostics_collection() {
        let mut diags = Diagnostics::new();
        assert!(!diags.has_errors());
        assert_eq!(diags.clone(), diags);

        let span = Span::new(0, 1, 1, 1, 1, 2);
        diags.push(Diagnostic::new(
            HclError::Parse("err".to_string()),
            span.clone(),
        ));
        assert!(diags.has_errors());

        let mut diags2 = Diagnostics::new();
        diags2.push(Diagnostic::warning("warn", "det", span));
        diags.extend(diags2);
        assert_eq!(diags.errors().len(), 2);
    }

    #[test]
    fn test_diagnostic_writer_plain_single_line() {
        let source = "foo = 123\nbar = 456\nbaz = 789\n";
        let span = Span::new(10, 19, 2, 1, 2, 10);
        let diag = Diagnostic::error("Invalid argument", "Argument 'bar' is not supported.", span);

        let writer = DiagnosticWriter::plain();
        let rendered = writer.format_diagnostic(&diag, "test.hcl", source);

        assert!(rendered.contains("Error: Invalid argument"));
        assert!(rendered.contains("on test.hcl line 2, col 1:"));
        assert!(rendered.contains("2 | bar = 456"));
        assert!(rendered.contains("  | ^^^^^^^^^"));
        assert!(rendered.contains("Argument 'bar' is not supported."));
    }

    #[test]
    fn test_diagnostic_writer_warning() {
        let source = "dep = true\n";
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let diag = Diagnostic::warning("Deprecated attribute", "Use 'new_dep' instead.", span);

        let writer = DiagnosticWriter::default();
        let rendered = writer.format_diagnostic(&diag, "", source);

        assert!(rendered.contains("Warning: Deprecated attribute"));
        assert!(rendered.contains("on line 1, col 1:"));
        assert!(rendered.contains("1 | dep = true"));
        assert!(rendered.contains("  | ^^^^^^^^^^"));
        assert!(rendered.contains("Use 'new_dep' instead."));
    }

    #[test]
    fn test_diagnostic_writer_multi_line() {
        let source = "block {\n  a = 1\n  b = 2\n}\n";
        let span = Span::new(8, 24, 2, 3, 4, 2);
        let diag = Diagnostic::new(HclError::Parse("multi-line issue".into()), span);

        let writer = DiagnosticWriter::new(false);
        let rendered = writer.format_diagnostic(&diag, "main.tf", source);

        assert!(rendered.contains("2 |   a = 1"));
        assert!(rendered.contains("3 |   b = 2"));
        assert!(rendered.contains("4 | }"));
    }

    #[test]
    fn test_diagnostic_writer_with_context() {
        let source = "attr = 1\n\n\n\n\nattr = 2\n";
        let ctx_span = Span::new(0, 8, 1, 1, 1, 9);
        let subj_span = Span::new(13, 21, 6, 1, 6, 9);

        let diag = Diagnostic::error(
            "Attribute redefined",
            "Each argument may be set once.",
            subj_span,
        )
        .with_context(ctx_span);

        let writer = DiagnosticWriter::plain();
        let rendered = writer.format_diagnostic(&diag, "file.hcl", source);

        assert!(rendered.contains("1 | attr = 1"));
        assert!(rendered.contains("  | ~~~~~~~~"));
        assert!(rendered.contains("| ..."));
        assert!(rendered.contains("6 | attr = 2"));
        assert!(rendered.contains("  | ^^^^^^^^"));
    }

    #[test]
    fn test_diagnostic_writer_same_line_context_and_subject() {
        let source = "foo = bar + baz\n";
        let ctx_span = Span::new(0, 3, 1, 1, 1, 4);
        let subj_span = Span::new(6, 9, 1, 7, 1, 10);

        let diag = Diagnostic::error("Type mismatch", "", subj_span).with_context(ctx_span);

        let writer = DiagnosticWriter::plain();
        let rendered = writer.format_diagnostic(&diag, "calc.hcl", source);

        assert!(rendered.contains("1 | foo = bar + baz"));
        assert!(rendered.contains("  | ~~~   ^^^"));
    }

    #[test]
    fn test_diagnostic_writer_colored() {
        let source = "x = y\n";
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let diag_err = Diagnostic::error("Error sum", "detail", span);
        let subj_span = Span::new(3, 5, 1, 4, 1, 6);
        let ctx_span = Span::new(0, 2, 1, 1, 1, 3);
        let diag_warn = Diagnostic::warning("Warn sum", "detail", subj_span).with_context(ctx_span);

        let writer = DiagnosticWriter::colored().with_color(true);
        assert!(writer.color);

        let rendered_err = writer.format_diagnostic(&diag_err, "test.hcl", source);
        assert!(rendered_err.contains("\x1b[1;31mError:\x1b[0m"));
        assert!(rendered_err.contains("\x1b[36m")); // gutter color
        assert!(rendered_err.contains("\x1b[1;31m^")); // subject red underline

        let rendered_warn = writer.format_diagnostic(&diag_warn, "test.hcl", source);
        assert!(rendered_warn.contains("\x1b[1;33mWarning:\x1b[0m"));
        assert!(rendered_warn.contains("\x1b[1;33m~")); // context yellow underline
    }

    #[test]
    fn test_diagnostic_writer_empty_source_and_out_of_bounds() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let diag = Diagnostic::new(HclError::Parse("generic error".into()), span);

        let writer = DiagnosticWriter::plain();
        let rendered = writer.format_diagnostic(&diag, "", "");
        assert!(rendered.contains("Error: Parse error: generic error"));

        let span_oob = Span::new(100, 200, 999, 1, 999, 5);
        let diag_oob = Diagnostic::new(HclError::Parse("oob error".into()), span_oob);
        let rendered_oob = writer.format_diagnostic(&diag_oob, "file.hcl", "short file");
        assert!(rendered_oob.contains("on file.hcl line 999, col 1:"));
    }

    #[test]
    fn test_diagnostic_writer_max_lines_truncation() {
        let mut source = String::new();
        for i in 1..=20 {
            source.push_str(&format!("line_{i} = {i}\n"));
        }
        let span = Span::new(0, 100, 1, 1, 15, 5);
        let diag = Diagnostic::error("Long block", "", span);

        let writer = DiagnosticWriter::plain().with_max_lines(3);
        assert_eq!(writer.max_lines, 3);
        let rendered = writer.format_diagnostic(&diag, "long.hcl", &source);
        assert!(rendered.contains("1 | line_1 = 1"));
        assert!(rendered.contains("2 | line_2 = 2"));
        assert!(rendered.contains("3 | line_3 = 3"));
        assert!(rendered.contains("| ..."));
    }

    #[test]
    fn test_diagnostic_writer_write_methods() {
        let source = "val = 1\n";
        let span = Span::new(0, 7, 1, 1, 1, 8);
        let first_err = Diagnostic::error("Err 1", "det 1", span.clone());
        let second_warn = Diagnostic::warning("Warn 2", "det 2", span);

        let mut diag_list = Diagnostics::new();
        diag_list.push(first_err.clone());
        diag_list.push(second_warn);

        let writer = DiagnosticWriter::plain();

        // format_diagnostics
        let formatted = writer.format_diagnostics(&diag_list, "test.hcl", source);
        assert!(formatted.contains("Err 1"));
        assert!(formatted.contains("Warn 2"));

        // write_diagnostic
        let mut buf_single = Vec::new();
        writer
            .write_diagnostic(&first_err, "test.hcl", source, &mut buf_single)
            .unwrap();
        assert_ne!(buf_single, Vec::<u8>::new());

        // write_diagnostics
        let mut buf_multi = Vec::new();
        writer
            .write_diagnostics(&diag_list, "test.hcl", source, &mut buf_multi)
            .unwrap();
        assert_ne!(buf_multi, Vec::<u8>::new());
    }

    #[test]
    fn test_diagnostic_writer_colored_gap_and_truncation() {
        let mut source = String::new();
        for i in 1..=10 {
            source.push_str(&format!("item_{i} = {i}\n"));
        }
        let ctx_span = Span::new(0, 10, 1, 1, 1, 10);
        let subj_span = Span::new(50, 60, 8, 1, 10, 5);

        let diag = Diagnostic::error("Gap error", "", subj_span).with_context(ctx_span);
        let writer = DiagnosticWriter::colored().with_max_lines(2);
        let rendered = writer.format_diagnostic(&diag, "gap.hcl", &source);

        // Verify color escape in gutter ellipsis
        assert!(rendered.contains("\x1b[36m"));
        assert!(rendered.contains("..."));
    }

    #[test]
    fn test_diagnostic_writer_intermediate_lines_and_style_transitions() {
        let source = "line1\nline2_intermediate\nline3\nline4\n";
        // 3-line span: line 1 to line 3
        let span_3line = Span::new(0, 25, 1, 1, 3, 5);
        let diag = Diagnostic::new(HclError::Parse("3-line".into()), span_3line);
        let writer = DiagnosticWriter::plain();
        let rendered = writer.format_diagnostic(&diag, "lines.hcl", source);

        assert!(rendered.contains("1 | line1"));
        assert!(rendered.contains("2 | line2_intermediate"));
        assert!(rendered.contains("3 | line3"));

        // Test style transitions in style_underline_colored with space between context and subject
        // and direct transitions between ~ and ^
        let chars = vec!['^', '^', ' ', ' ', '~', '~', ' ', '^', '~', '^'];
        let styled = style_underline_colored(&chars);
        assert!(styled.contains("\x1b[1;31m^^"));
        assert!(styled.contains("\x1b[1;33m~~"));
        assert!(styled.contains("\x1b[0m"));

        assert_eq!(style_underline_colored(&[]), "");
        assert_eq!(style_underline_colored(&[' ']), " ");
        let chars_trailing_space = vec!['^', ' '];
        let styled_space = style_underline_colored(&chars_trailing_space);
        assert!(styled_space.contains("\x1b[0m"));
    }

    #[test]
    fn test_diagnostic_coverage_branches() {
        let span = Span::new(0, 1, 1, 1, 1, 2);
        let diag = Diagnostic::new(HclError::Parse("single".into()), span);
        let diags: Diagnostics = diag.into();
        assert_eq!(diags.errors().len(), 1);

        let writer = DiagnosticWriter::plain();

        // start_line == 0 with non-empty source
        let diag_zero = Diagnostic::error("zero line", "", Span::new(0, 0, 0, 0, 0, 0));
        let rendered_zero = writer.format_diagnostic(&diag_zero, "zero.hcl", "some text\n");
        assert!(rendered_zero.contains("zero line"));

        // context with start_line == 0
        let diag_ctx_zero = Diagnostic::error("ctx zero", "", Span::new(0, 1, 1, 1, 1, 2))
            .with_context(Span::new(0, 0, 0, 0, 0, 0));
        let rendered_ctx_zero = writer.format_diagnostic(&diag_ctx_zero, "file.hcl", "single line");
        assert!(rendered_ctx_zero.contains("1 | single line"));

        // context with start_line > lines.len()
        let diag_ctx_oob = Diagnostic::error("ctx oob", "", Span::new(0, 1, 1, 1, 1, 2))
            .with_context(Span::new(100, 200, 999, 1, 999, 5));
        let rendered_ctx_oob = writer.format_diagnostic(&diag_ctx_oob, "file.hcl", "single line");
        assert!(rendered_ctx_oob.contains("1 | single line"));

        // multi-line with empty intermediate line
        let source = "line1\n\nline3\n";
        let span_empty_line = Span::new(0, 13, 1, 1, 3, 5);
        let diag_empty = Diagnostic::new(HclError::Parse("empty middle".into()), span_empty_line);
        let rendered_empty = writer.format_diagnostic(&diag_empty, "empty.hcl", source);
        assert!(rendered_empty.contains("1 | line1"));
        assert!(rendered_empty.contains("3 | line3"));
    }

    #[test]
    fn test_diagnostic_writer_format_value_and_diff() {
        use crate::types::{Type, Value, ValueData, ValueMark};

        let plain_writer = DiagnosticWriter::plain();
        let colored_writer = DiagnosticWriter::colored();

        let v_plain = Value::new(Type::String, ValueData::String("database_url".into()));
        let v_sens = Value::new(Type::String, ValueData::String("secret_pass".into()))
            .mark(ValueMark::Sensitive);

        assert_eq!(plain_writer.format_value(&v_plain), "\"database_url\"");
        assert_eq!(plain_writer.format_value(&v_sens), "(sensitive value)");

        // Plain diff
        let diff_plain = plain_writer.format_diff(&v_plain, &v_sens);
        assert_eq!(diff_plain, "- \"database_url\"\n+ (sensitive value)");

        // Colored diff
        let diff_colored = colored_writer.format_diff(&v_sens, &v_plain);
        assert!(diff_colored.contains("\x1b[31m- (sensitive value)\x1b[0m"));
        assert!(diff_colored.contains("\x1b[32m+ \"database_url\"\x1b[0m"));
    }

    #[test]
    fn test_diagnostic_file_tracking_and_multi_file_rendering() {
        let file_main: std::sync::Arc<str> = std::sync::Arc::from("main.tf");
        let file_module: std::sync::Arc<str> = std::sync::Arc::from("module.tf");

        let subj_span = Span::new_with_file(0, 10, 3, 5, 3, 15, Some(file_main));
        let ctx_span = Span::new_with_file(0, 10, 1, 1, 1, 10, Some(file_module));

        let diag = Diagnostic::error("Invalid resource", "Resource type not found", subj_span)
            .with_context(ctx_span);

        let writer = DiagnosticWriter::plain();
        let rendered = writer.format_diagnostic(&diag, "", "line 1\nline 2\nline 3\n");

        // Asserts filename from subject.file is used when filename parameter is empty
        assert!(rendered.contains("on main.tf line 3, col 5:"));
        // Asserts multi-file context header is rendered
        assert!(rendered.contains("(context in module.tf line 1, col 1):"));
    }

    struct StepWriter {
        remaining: usize,
    }

    impl std::fmt::Write for StepWriter {
        fn write_str(&mut self, _: &str) -> std::fmt::Result {
            if self.remaining == 0 {
                Err(std::fmt::Error)
            } else {
                self.remaining = self.remaining.saturating_sub(1);
                Ok(())
            }
        }
    }

    #[test]
    fn test_diagnostics_iter_and_display_error_paths() {
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let d1 = Diagnostic::error("E1", "detail 1", span.clone());
        let d2 = Diagnostic::warning("W2", "detail 2", span.clone());
        let d3 = Diagnostic::error("E3", "", span);
        let mut diags = Diagnostics::new();
        diags.push(d1);
        diags.push(d2);
        diags.push(d3);

        // Test iter() and &Diagnostics into_iter()
        assert_eq!(diags.iter().count(), 3);
        let mut count_into = 0;
        for _ in &diags {
            count_into += 1;
        }
        assert_eq!(count_into, 3);

        // Display string with mixed Some and None details
        let display_str = diags.to_string();
        assert!(display_str.contains("E1: detail 1"));
        assert!(display_str.contains("E3"));

        // Test display error paths
        for r in 0..10 {
            let mut w = StepWriter { remaining: r };
            let _ = std::fmt::write(&mut w, format_args!("{diags}"));
        }
    }

    #[test]
    fn test_eval_callout_sensitive_masking() {
        use crate::types::{Type, Value, ValueData, ValueMark};

        let span = Span::new(0, 10, 1, 1, 1, 11);

        // Plain value
        let val_plain = Value::new(Type::String, ValueData::String("public_info".into()));
        let callout_plain = EvalCallout::from_value(span.clone(), "var.info", &val_plain);
        assert_eq!(callout_plain.expression_text, "var.info");
        assert_eq!(callout_plain.evaluated_value, "\"public_info\"");

        // Sensitive value is masked automatically
        let val_sens = Value::new(Type::String, ValueData::String("super_secret".into()))
            .mark(ValueMark::Sensitive);
        let callout_sens = EvalCallout::from_value(span.clone(), "var.secret", &val_sens);
        assert_eq!(callout_sens.expression_text, "var.secret");
        assert_eq!(callout_sens.evaluated_value, "(sensitive value)");
    }

    #[test]
    fn test_diagnostic_eval_callouts_rendering_plain_and_colored() {
        let span = Span::new(0, 20, 2, 5, 2, 25);
        let diag = Diagnostic::error("Type error", "Cannot add incompatible types", span.clone())
            .with_callout(EvalCallout::new(span.clone(), "var.count", "10"))
            .with_callout(EvalCallout::new(span, "var.token", "(sensitive value)"));

        assert_eq!(diag.eval_callouts.len(), 2);

        let source = "line 1\n    val = var.count + var.token\nline 3\n";
        let plain_writer = DiagnosticWriter::plain();
        let rendered_plain = plain_writer.format_diagnostic(&diag, "test.hcl", source);

        // Asserts tree connectors ├─ and └─
        assert!(rendered_plain.contains("├─ var.count is 10"));
        assert!(rendered_plain.contains("└─ var.token is (sensitive value)"));

        let colored_writer = DiagnosticWriter::colored();
        let rendered_colored = colored_writer.format_diagnostic(&diag, "test.hcl", source);
        assert!(rendered_colored.contains("├─ \x1b[1mvar.count\x1b[0m is 10"));
        assert!(rendered_colored.contains("└─ \x1b[1mvar.token\x1b[0m is (sensitive value)"));

        let callout1 = EvalCallout::new(Span::default(), "a", "1");
        let callout2 = EvalCallout::new(Span::default(), "b", "2");
        let diag_zero = Diagnostic::error("Zero Line", "Message", Span::default())
            .with_callouts(vec![callout1, callout2]);
        let rendered_zero = plain_writer.format_diagnostic(&diag_zero, "test.hcl", source);
        assert!(rendered_zero.contains("├─ a is 1"));
        assert!(rendered_zero.contains("└─ b is 2"));
    }

    /// Tests uncovered helper methods, JSON conversions, and context rendering branches.
    #[test]
    fn test_diagnostic_coverage_gaps() {
        let span = Span::new_with_file(0, 10, 1, 1, 1, 11, Some(std::sync::Arc::from("main.tf")));
        let diag = Diagnostic::error("summary only", "", span.clone());
        assert_eq!(diag.detail, None);
        assert_eq!(diag.summary_str(), "summary only");

        // String variant for error with empty detail
        let diag_string = Diagnostic::error("sum".to_string(), String::new(), span.clone());
        assert_eq!(diag_string.detail, None);

        // Diagnostic with summary = None for summary_str() fallback
        let mut diag_no_summary =
            Diagnostic::new(HclError::Parse("err text".to_string()), span.clone());
        diag_no_summary.summary = None;
        assert_eq!(diag_no_summary.summary_str(), "Parse error: err text");

        // to_json_value and to_json
        let json_val = diag.to_json_value(Some("code line\n"));
        assert!(json_val.is_object());
        let json_str = diag.to_json(Some("code line\n")).unwrap();
        assert!(json_str.contains("summary only"));

        // with_address and Diagnostic::from_json
        let d_addr = diag.clone().with_address("module.foo");
        assert_eq!(d_addr.address.as_deref(), Some("module.foo"));

        let from_j = Diagnostic::from_json(&json_str).unwrap();
        assert_eq!(from_j.summary.as_deref(), Some("summary only"));
        assert!(Diagnostic::from_json("invalid json").is_err());

        // Diagnostics collection to_json_value, from_json
        let mut diags = Diagnostics::new();
        diags.push(diag.clone());
        let arr_val = diags.to_json_value(Some("code line\n"));
        assert!(arr_val.is_array());

        let diags_json = diags.to_json(Some("code line\n")).unwrap();
        let parsed_diags = Diagnostics::from_json(&diags_json).unwrap();
        assert_eq!(parsed_diags.errors().len(), 1);

        // Context with same file as subject (evaluates line 608 Some(ctx_file) != diag.subject.file to false)
        let mut diag_same_file = diag;
        let mut ctx_span = Span::new(0, 5, 1, 1, 1, 6);
        ctx_span.file = Some(std::sync::Arc::from("main.tf"));
        diag_same_file.context = Some(ctx_span);
        let rendered =
            DiagnosticWriter::plain().format_diagnostic(&diag_same_file, "main.tf", "code line\n");
        assert!(!rendered.contains("(context in"));

        // HTML serialization tests
        let html_str = diags.to_html(Some("code line\n"), true);
        assert!(html_str.contains("<!DOCTYPE html>"));
        assert!(html_str.contains("summary only"));

        let mut html_buf = Vec::new();
        assert!(
            diags
                .write_html(&mut html_buf, Some("code line\n"), false)
                .is_ok()
        );
        let snippet = String::from_utf8_lossy(&html_buf);
        assert!(!snippet.contains("<!DOCTYPE html>"));
        assert!(snippet.contains("summary only"));
    }
}
