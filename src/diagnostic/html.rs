//! HTML diagnostic report generator matching `HashiCorp` HCL's `DiagnosticHTMLWriter`.
//!
//! Renders diagnostics into self-contained HTML documents or embeddable HTML snippets
//! with syntax highlighting, line gutters, error underlines, and responsive CSS styling.

use crate::diagnostic::{Diagnostic, Diagnostics, Severity};
use std::fmt::Write as FmtWrite;
use std::io::Write;

/// HTML entity escaping helper to prevent XSS.
#[must_use]
pub fn html_escape(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// Default responsive CSS stylesheet supporting both light and dark display modes.
pub const DEFAULT_CSS: &str = r#"
:root {
  --bg-color: #ffffff;
  --text-color: #24292f;
  --card-bg: #f6f8fa;
  --card-border: #d0d7de;
  --gutter-color: #6e7781;
  --error-bg: #ffebe9;
  --error-border: #cf222e;
  --error-badge: #cf222e;
  --warning-bg: #fff8c5;
  --warning-border: #9a6700;
  --warning-badge: #9a6700;
  --code-bg: #f6f8fa;
  --underline-err: #cf222e;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg-color: #0d1117;
    --text-color: #c9d1d9;
    --card-bg: #161b22;
    --card-border: #30363d;
    --gutter-color: #8b949e;
    --error-bg: #490202;
    --error-border: #f85149;
    --error-badge: #f85149;
    --warning-bg: #3d2e00;
    --warning-border: #d29922;
    --warning-badge: #d29922;
    --code-bg: #0d1117;
    --underline-err: #f85149;
  }
}
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
  background-color: var(--bg-color);
  color: var(--text-color);
  margin: 0;
  padding: 24px;
}
.diag-container {
  max-width: 900px;
  margin: 0 auto;
}
.diag-card {
  border: 1px solid var(--card-border);
  border-radius: 6px;
  background-color: var(--card-bg);
  margin-bottom: 16px;
  overflow: hidden;
}
.diag-header {
  padding: 12px 16px;
  display: flex;
  align-items: center;
  gap: 8px;
  border-bottom: 1px solid var(--card-border);
}
.diag-card.error .diag-header {
  background-color: var(--error-bg);
  border-bottom-color: var(--error-border);
}
.diag-card.warning .diag-header {
  background-color: var(--warning-bg);
  border-bottom-color: var(--warning-border);
}
.badge {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  padding: 2px 6px;
  border-radius: 4px;
  color: #ffffff;
}
.badge-error { background-color: var(--error-badge); }
.badge-warning { background-color: var(--warning-badge); }
.diag-summary {
  font-weight: 600;
  font-size: 14px;
}
.diag-body {
  padding: 16px;
}
.diag-location {
  font-size: 12px;
  color: var(--gutter-color);
  margin-bottom: 8px;
}
.diag-snippet {
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
  background-color: var(--code-bg);
  border: 1px solid var(--card-border);
  border-radius: 4px;
  padding: 8px 0;
  overflow-x: auto;
}
.snippet-line {
  display: flex;
  padding: 0 12px;
}
.snippet-line.highlight {
  background-color: var(--error-bg);
}
.gutter {
  color: var(--gutter-color);
  user-select: none;
  min-width: 40px;
  text-align: right;
  padding-right: 16px;
}
.code {
  white-space: pre;
}
.error-underline {
  text-decoration: underline wavy var(--underline-err);
  font-weight: bold;
}
.diag-detail {
  margin-top: 12px;
  font-size: 13px;
  white-space: pre-wrap;
}
.callouts-list {
  margin-top: 8px;
  padding-left: 20px;
  font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  font-size: 12px;
}
"#;

/// Configurable HTML diagnostic report writer.
#[derive(Debug, Clone)]
pub struct HtmlDiagnosticWriter {
    /// Whether to generate a standalone HTML document or an embeddable snippet.
    pub standalone: bool,
    /// HTML document title.
    pub title: String,
    /// Custom CSS stylesheet string.
    pub custom_css: Option<String>,
}

impl Default for HtmlDiagnosticWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl HtmlDiagnosticWriter {
    /// Creates a new `HtmlDiagnosticWriter` with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            standalone: true,
            title: "HCL Diagnostics Report".to_string(),
            custom_css: None,
        }
    }

    /// Sets whether the output should be a standalone HTML document.
    ///
    /// # Arguments
    /// * `standalone` - `true` for full HTML document, `false` for snippet.
    #[must_use]
    pub fn with_standalone(mut self, standalone: bool) -> Self {
        self.standalone = standalone;
        self
    }

    /// Sets the document title for standalone HTML reports.
    ///
    /// # Arguments
    /// * `title` - The HTML title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets custom CSS stylesheet contents.
    ///
    /// # Arguments
    /// * `css` - Custom CSS stylesheet.
    #[must_use]
    pub fn with_css(mut self, css: impl Into<String>) -> Self {
        self.custom_css = Some(css.into());
        self
    }

    /// Renders a collection of diagnostics to an HTML string.
    ///
    /// # Arguments
    /// * `diagnostics` - The diagnostics to render.
    /// * `source` - Optional source file content for code snippet extraction.
    #[must_use]
    pub fn render_to_string(&self, diagnostics: &Diagnostics, source: Option<&str>) -> String {
        let mut out = String::new();
        if self.standalone {
            out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
            let _ = writeln!(&mut out, "  <title>{}</title>", html_escape(&self.title));
            let css = self.custom_css.as_deref().unwrap_or(DEFAULT_CSS);
            let _ = writeln!(&mut out, "  <style>{css}</style>\n</head>\n<body>");
        }

        out.push_str("<div class=\"diag-container\">\n");
        for diag in diagnostics.errors() {
            Self::write_diagnostic_card(diag, source, &mut out);
        }
        out.push_str("</div>\n");

        if self.standalone {
            out.push_str("</body>\n</html>\n");
        }

        out
    }

    /// Writes the rendered HTML report to an arbitrary output stream.
    ///
    /// # Arguments
    /// * `diagnostics` - The diagnostics to render.
    /// * `source` - Optional source file content for code snippet extraction.
    /// * `writer` - The output stream.
    ///
    /// # Errors
    /// Returns [`std::io::Error`] if writing fails.
    pub fn write_html(
        &self,
        diagnostics: &Diagnostics,
        source: Option<&str>,
        writer: &mut dyn Write,
    ) -> std::io::Result<()> {
        let content = self.render_to_string(diagnostics, source);
        writer.write_all(content.as_bytes())
    }

    fn write_diagnostic_card(diag: &Diagnostic, source: Option<&str>, out: &mut String) {
        let (card_cls, badge_cls, badge_text) = match diag.severity {
            Severity::Error => ("diag-card error", "badge badge-error", "Error"),
            Severity::Warning => ("diag-card warning", "badge badge-warning", "Warning"),
        };

        let _ = writeln!(out, "  <div class=\"{card_cls}\">");
        let _ = writeln!(out, "    <div class=\"diag-header\">");
        let _ = writeln!(out, "      <span class=\"{badge_cls}\">{badge_text}</span>");
        let _ = writeln!(
            out,
            "      <span class=\"diag-summary\">{}</span>",
            html_escape(&diag.summary_str())
        );
        let _ = writeln!(out, "    </div>");

        let _ = writeln!(out, "    <div class=\"diag-body\">");

        // Location info
        let file_name = diag.subject.file.as_deref().unwrap_or("<unknown>");
        let _ = writeln!(
            out,
            "      <div class=\"diag-location\">on {}:{}:{}</div>",
            html_escape(file_name),
            diag.subject.start_line,
            diag.subject.start_col
        );

        // Source snippet rendering
        if let Some(src) = source {
            Self::write_code_snippet(diag, src, out);
        }

        // Diagnostic detail
        if let Some(ref detail) = diag.detail {
            let _ = writeln!(
                out,
                "      <div class=\"diag-detail\">{}</div>",
                html_escape(detail)
            );
        }

        // Sub-expression evaluation callouts
        if !diag.eval_callouts.is_empty() {
            let _ = writeln!(out, "      <ul class=\"callouts-list\">");
            for callout in &diag.eval_callouts {
                let _ = writeln!(
                    out,
                    "        <li><code>{}</code> = <code>{}</code></li>",
                    html_escape(&callout.expression_text),
                    html_escape(&callout.evaluated_value)
                );
            }
            let _ = writeln!(out, "      </ul>");
        }

        let _ = writeln!(out, "    </div>");
        let _ = writeln!(out, "  </div>");
    }

    fn write_code_snippet(diag: &Diagnostic, source: &str, out: &mut String) {
        let lines: Vec<&str> = source.lines().collect();
        let target_line_idx = diag.subject.start_line.saturating_sub(1);
        if target_line_idx >= lines.len() {
            return;
        }

        let start_idx = target_line_idx.saturating_sub(1);
        let end_idx = (target_line_idx + 2).min(lines.len());

        let _ = writeln!(out, "      <div class=\"diag-snippet\">");
        for (idx, line_text) in lines[start_idx..end_idx].iter().enumerate() {
            let current_line_num = start_idx + idx + 1;
            let is_target = current_line_num == diag.subject.start_line;
            let line_cls = if is_target {
                "snippet-line highlight"
            } else {
                "snippet-line"
            };

            let _ = writeln!(out, "        <div class=\"{line_cls}\">");
            let _ = writeln!(
                out,
                "          <span class=\"gutter\">{current_line_num}</span>"
            );

            if is_target {
                let col_start = diag.subject.start_col.saturating_sub(1);
                let col_end = diag.subject.end_col.saturating_sub(1).max(col_start + 1);
                let chars: Vec<char> = line_text.chars().collect();

                let prefix: String = chars.iter().take(col_start).collect();
                let underline: String = chars
                    .iter()
                    .skip(col_start)
                    .take(col_end.saturating_sub(col_start))
                    .collect();
                let suffix: String = chars.iter().skip(col_end).collect();

                let _ = writeln!(
                    out,
                    "          <span class=\"code\">{}<span class=\"error-underline\">{}</span>{}</span>",
                    html_escape(&prefix),
                    html_escape(&underline),
                    html_escape(&suffix)
                );
            } else {
                let _ = writeln!(
                    out,
                    "          <span class=\"code\">{}</span>",
                    html_escape(line_text)
                );
            }
            let _ = writeln!(out, "        </div>");
        }
        let _ = writeln!(out, "      </div>");
    }
}

/// Convenience function rendering diagnostics directly to an HTML string.
///
/// # Arguments
/// * `diagnostics` - Diagnostics to render.
/// * `source` - Optional source file content for snippet generation.
/// * `standalone` - Whether to generate a complete standalone HTML document.
#[must_use]
pub fn diagnostics_to_html(
    diagnostics: &Diagnostics,
    source: Option<&str>,
    standalone: bool,
) -> String {
    HtmlDiagnosticWriter::new()
        .with_standalone(standalone)
        .render_to_string(diagnostics, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{EvalCallout, Span};
    use crate::error::HclError;

    #[test]
    fn test_html_escape() {
        assert_eq!(
            html_escape(r#"foo & bar < baz > " '"#),
            "foo &amp; bar &lt; baz &gt; &quot; &#39;"
        );
        assert_eq!(html_escape("clean"), "clean");
    }

    #[test]
    fn test_diagnostics_to_html_standalone() {
        let mut diags = Diagnostics::new();
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let mut d = Diagnostic::error("Unexpected token", "Expected identifier here", span.clone());
        d.eval_callouts.push(EvalCallout {
            span,
            expression_text: "var.env".to_string(),
            evaluated_value: r#""prod""#.to_string(),
        });
        diags.push(d);

        let html = diagnostics_to_html(&diags, Some("var.env = 123\n"), true);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Unexpected token"));
        assert!(html.contains("Expected identifier here"));
        assert!(html.contains("var.env"));
        assert!(html.contains("&quot;prod&quot;"));
        assert!(html.contains("snippet-line highlight"));
        assert!(html.contains("error-underline"));
    }

    #[test]
    fn test_diagnostics_to_html_snippet_and_warning() {
        let mut diags = Diagnostics::new();
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let d = Diagnostic::warning("Deprecated syntax", "Use modern syntax instead", span);
        diags.push(d);

        let writer = HtmlDiagnosticWriter::default()
            .with_standalone(false)
            .with_title("Custom Title")
            .with_css("body { color: red; }");
        let html = writer.render_to_string(&diags, None);

        assert!(!html.contains("<!DOCTYPE html>"));
        assert!(html.contains("badge-warning"));
        assert!(html.contains("Deprecated syntax"));
    }

    #[test]
    fn test_diagnostics_to_html_out_of_bounds_source() {
        let mut diags = Diagnostics::new();
        let span = Span::new(100, 110, 999, 1, 999, 5);
        diags.push(Diagnostic::error(
            HclError::Parse("overflow".into()).to_string(),
            "",
            span,
        ));

        let html = diagnostics_to_html(&diags, Some("line 1\n"), false);
        assert!(html.contains("diag-card error"));
    }

    #[test]
    fn test_diagnostics_to_html_multiline_context_and_custom_css() {
        let mut diags = Diagnostics::new();
        let mut span = Span::new(7, 13, 2, 1, 2, 7);
        span.file = Some(std::sync::Arc::from("config.hcl"));

        let mut d = Diagnostic::error("Invalid value", "", span);
        d.detail = None; // Test detail = None branch
        diags.push(d);

        let writer = HtmlDiagnosticWriter::new()
            .with_standalone(true)
            .with_css("body { margin: 0; }")
            .with_title("Custom Doc");

        let source = "line 1\ntarget line\nline 3\n";
        let mut buf = Vec::new();
        assert!(writer.write_html(&diags, Some(source), &mut buf).is_ok());

        let html = String::from_utf8_lossy(&buf);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>Custom Doc</title>"));
        assert!(html.contains("<style>body { margin: 0; }</style>"));
        assert!(html.contains("config.hcl:2:1"));
        assert!(html.contains("gutter\">1<"));
        assert!(html.contains("gutter\">2<"));
        assert!(html.contains("gutter\">3<"));
        assert!(html.contains("snippet-line highlight"));
    }

    #[test]
    fn test_diagnostics_to_html_warning_with_snippet() {
        let mut diags = Diagnostics::new();
        let mut span = Span::new(0, 4, 1, 1, 1, 5);
        span.file = Some(std::sync::Arc::from("warn.hcl"));
        let mut warn = Diagnostic::warning("Warning title", "Warning summary", span);
        warn.detail = Some("Detailed explanation of warning".to_string());
        diags.push(warn);

        let html = diagnostics_to_html(&diags, Some("warn line\n"), false);
        assert!(html.contains("badge badge-warning"));
        assert!(html.contains("Detailed explanation of warning"));
        assert!(html.contains("warn.hcl:1:1"));
    }

    struct FailWriter;
    impl std::io::Write for FailWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("write error"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("flush error"))
        }
    }

    #[test]
    fn test_diagnostics_to_html_io_error() {
        let mut fail = FailWriter;
        assert!(fail.flush().is_err());

        let mut diags = Diagnostics::new();
        diags.push(Diagnostic::error("fail", "summary", Span::default()));
        let writer = HtmlDiagnosticWriter::new().with_standalone(true);
        assert!(
            writer
                .write_html(&diags, Some("code\n"), &mut fail)
                .is_err()
        );

        let snippet_writer = HtmlDiagnosticWriter::new().with_standalone(false);
        assert!(
            snippet_writer
                .write_html(&diags, Some("code\n"), &mut fail)
                .is_err()
        );
    }
}
