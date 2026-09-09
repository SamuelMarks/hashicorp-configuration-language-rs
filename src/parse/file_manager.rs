//! Multi-file parser cache and file manager.
//!
//! Provides [`HclParser`](crate::parse::file_manager::HclParser) (and its alias [`FileManager`](crate::parse::file_manager::FileManager)) for loading, caching,
//! and merging HCL and JSON configuration files across projects.

use crate::ast::structure::Body;
use crate::diagnostic::{Diagnostic, DiagnosticWriter, Diagnostics};
use crate::error::HclError;
use crate::parse::json::parse_json;
use crate::parse::merge::merge_bodies;
use crate::parse::parser::Parser;
use crate::span::Span;
use std::collections::BTreeMap;
use std::fs;

/// In-memory cache and manager for multiple HCL and JSON source files.
#[derive(Debug, Clone, Default)]
pub struct HclParser {
    /// In-memory map of filenames to source contents and parsed bodies.
    files: BTreeMap<String, (String, Body)>,
    /// Map of buffer IDs to canonical file paths.
    buffer_paths: BTreeMap<usize, String>,
}

/// Type alias for [`HclParser`].
pub type FileManager = HclParser;

impl HclParser {
    /// Creates a new, empty [`HclParser`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            buffer_paths: BTreeMap::new(),
        }
    }

    /// Associates a numeric buffer ID with a canonical file path.
    ///
    /// # Arguments
    /// * `buffer_id` - The numeric identifier for the buffer.
    /// * `path` - The canonical file path.
    pub fn associate_buffer_path(&mut self, buffer_id: usize, path: impl Into<String>) {
        self.buffer_paths.insert(buffer_id, path.into());
    }

    /// Resolves a canonical file path for a numeric buffer ID if associated.
    ///
    /// # Arguments
    /// * `buffer_id` - The buffer ID to look up.
    #[must_use]
    pub fn get_buffer_path(&self, buffer_id: usize) -> Option<&str> {
        self.buffer_paths.get(&buffer_id).map(String::as_str)
    }

    /// Loads and parses an HCL file from the filesystem.
    ///
    /// # Arguments
    /// * `path` - The path to the HCL file.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if file reading or parsing fails.
    pub fn parse_hcl_file(&mut self, path: &str) -> Result<&Body, Diagnostics> {
        let content = fs::read_to_string(path).map_err(|err| {
            let mut diags = Diagnostics::new();
            diags.push(
                Diagnostic::error(
                    "Failed to read file",
                    format!("Could not read file '{path}': {err}"),
                    Span::new(0, 0, 1, 1, 1, 1),
                )
                .with_error(HclError::Io(err.to_string())),
            );
            diags
        })?;

        self.parse_hcl_string(path, &content)
    }

    /// Parses an in-memory HCL string and stores it under `filename`.
    ///
    /// # Arguments
    /// * `filename` - The name or path representing this source buffer.
    /// * `content` - The HCL source text to parse.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if parsing fails.
    pub fn parse_hcl_string(
        &mut self,
        filename: &str,
        content: &str,
    ) -> Result<&Body, Diagnostics> {
        let file_arc = Some(std::sync::Arc::from(filename));
        let mut parser = Parser::new_with_file(content, file_arc);
        let body = parser.parse_body();

        if parser.errors().has_errors() {
            return Err(parser.errors().clone());
        }

        use std::collections::btree_map::Entry;
        let body_ref = match self.files.entry(filename.to_string()) {
            Entry::Vacant(v) => &v.insert((content.to_string(), body)).1,
            Entry::Occupied(mut o) => {
                o.insert((content.to_string(), body));
                &o.into_mut().1
            }
        };
        Ok(body_ref)
    }

    /// Loads and parses a JSON configuration file from the filesystem.
    ///
    /// # Arguments
    /// * `path` - The path to the JSON file.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if file reading or JSON parsing fails.
    pub fn parse_json_file(&mut self, path: &str) -> Result<&Body, Diagnostics> {
        let content = fs::read_to_string(path).map_err(|err| {
            let mut diags = Diagnostics::new();
            diags.push(
                Diagnostic::error(
                    "Failed to read file",
                    format!("Could not read file '{path}': {err}"),
                    Span::new(0, 0, 1, 1, 1, 1),
                )
                .with_error(HclError::Io(err.to_string())),
            );
            diags
        })?;

        self.parse_json_string(path, &content)
    }

    /// Parses an in-memory JSON string and stores it under `filename`.
    ///
    /// # Arguments
    /// * `filename` - The name or path representing this source buffer.
    /// * `content` - The JSON source text to parse.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if JSON parsing fails.
    pub fn parse_json_string(
        &mut self,
        filename: &str,
        content: &str,
    ) -> Result<&Body, Diagnostics> {
        let body = parse_json(content)?;

        use std::collections::btree_map::Entry;
        let body_ref = match self.files.entry(filename.to_string()) {
            Entry::Vacant(v) => &v.insert((content.to_string(), body)).1,
            Entry::Occupied(mut o) => {
                o.insert((content.to_string(), body));
                &o.into_mut().1
            }
        };
        Ok(body_ref)
    }

    /// Returns a reference to all loaded files in sorted filename order.
    #[must_use]
    pub fn files(&self) -> &BTreeMap<String, (String, Body)> {
        &self.files
    }

    /// Returns the source content and parsed body for the given filename, if loaded.
    ///
    /// # Arguments
    /// * `filename` - The name of the file to retrieve.
    #[must_use]
    pub fn get_file(&self, filename: &str) -> Option<&(String, Body)> {
        self.files.get(filename)
    }

    /// Returns the parsed body for the given filename, if loaded.
    ///
    /// # Arguments
    /// * `filename` - The name of the file to retrieve.
    #[must_use]
    pub fn get_body(&self, filename: &str) -> Option<&Body> {
        self.files.get(filename).map(|(_, b)| b)
    }

    /// Returns the source content for the given filename, if loaded.
    ///
    /// # Arguments
    /// * `filename` - The name of the file to retrieve.
    #[must_use]
    pub fn get_source(&self, filename: &str) -> Option<&str> {
        self.files.get(filename).map(|(s, _)| s.as_str())
    }

    /// Returns a list of all loaded filenames.
    #[must_use]
    pub fn filenames(&self) -> Vec<&str> {
        self.files.keys().map(String::as_str).collect()
    }

    /// Returns the number of loaded files.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Returns true if no files have been loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Clears all loaded files from the cache.
    pub fn clear(&mut self) {
        self.files.clear();
    }

    /// Combines all parsed bodies across all loaded files into a single unified [`Body`].
    ///
    /// The files are merged in alphabetical order of their filenames.
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if attribute collisions or merging errors occur.
    pub fn merge_all(&self) -> Result<Body, Diagnostics> {
        let bodies: Vec<Body> = self.files.values().map(|(_, b)| b.clone()).collect();
        merge_bodies(bodies)
    }

    /// Formats diagnostics for a specific loaded file using a [`DiagnosticWriter`].
    ///
    /// # Arguments
    /// * `filename` - The filename to associate with the source code.
    /// * `diags` - The diagnostics to format.
    /// * `writer` - The diagnostic writer to use for formatting.
    #[must_use]
    pub fn format_diagnostics(
        &self,
        filename: &str,
        diags: &Diagnostics,
        writer: &DiagnosticWriter,
    ) -> String {
        let source = self.get_source(filename).map_or("", |s| s);
        writer.format_diagnostics(diags, filename, source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::context::Context;
    use crate::eval::evaluator::Evaluator;
    use std::io::Write;

    #[test]
    fn test_file_manager_hcl_and_json_strings() {
        let mut mgr = HclParser::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.file_count(), 0);
        assert_eq!(mgr.filenames(), Vec::<&str>::new());

        let hcl_src = "foo = 10
bar = 20
";
        let body = mgr.parse_hcl_string("vars.hcl", hcl_src).expect("parsed");
        assert_eq!(body.attributes.len(), 2);
        assert_eq!(mgr.file_count(), 1);
        assert_eq!(mgr.filenames(), vec!["vars.hcl"]);
        assert_eq!(mgr.get_source("vars.hcl"), Some(hcl_src));
        assert!(mgr.get_body("vars.hcl").is_some());
        assert!(mgr.get_file("vars.hcl").is_some());
        assert_eq!(mgr.get_file("nonexistent"), None);

        let json_src = r#"{"baz": 30}"#;
        let json_body = mgr
            .parse_json_string("extra.json", json_src)
            .expect("parsed json");
        assert_eq!(json_body.attributes.len(), 1);
        assert_eq!(mgr.file_count(), 2);
        assert_eq!(mgr.filenames(), vec!["extra.json", "vars.hcl"]);

        // Merged evaluation
        let merged = mgr.merge_all().expect("merge ok");
        assert_eq!(merged.attributes.len(), 3);

        // Re-parsing existing file (Occupied entry branch)
        let updated_body = mgr
            .parse_hcl_string("vars.hcl", "foo = 15\n")
            .expect("updated hcl");
        assert_eq!(updated_body.attributes.len(), 1);
        let updated_json = mgr
            .parse_json_string("extra.json", r#"{"baz": 35}"#)
            .expect("updated json");
        assert_eq!(updated_json.attributes.len(), 1);

        let mut ctx = Context::new();
        for (k, attr) in &merged.attributes {
            let (val, _) = Evaluator::new(&ctx).evaluate(&attr.expr).expect("eval");
            ctx.set_variable(k, val);
        }
        assert!(ctx.get_variable("foo").is_some());
        assert!(ctx.get_variable("bar").is_some());
        assert!(ctx.get_variable("baz").is_some());

        // Format diagnostics
        let diags = Diagnostics::from(Diagnostic::error(
            "Test error",
            "detail",
            Span::new(0, 3, 1, 1, 1, 4),
        ));
        let formatted = mgr.format_diagnostics("vars.hcl", &diags, &DiagnosticWriter::plain());
        assert!(formatted.contains("on vars.hcl line 1, col 1:"));

        let formatted_unk =
            mgr.format_diagnostics("unknown.hcl", &diags, &DiagnosticWriter::plain());
        assert!(formatted_unk.contains("Test error"));

        mgr.clear();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_file_manager_duplicate_collision() {
        let mut mgr = HclParser::new();
        mgr.parse_hcl_string(
            "a.hcl",
            "port = 80
",
        )
        .expect("ok");
        mgr.parse_hcl_string(
            "b.hcl",
            "port = 8080
",
        )
        .expect("ok");

        let err = mgr
            .merge_all()
            .expect_err("should have duplicate attribute error");
        assert_eq!(err.errors().len(), 1);
        assert!(
            err.errors()[0]
                .to_string()
                .contains("defined multiple times")
        );
    }

    #[test]
    fn test_file_manager_parse_errors() {
        let mut mgr = HclParser::new();
        let bad_hcl = "port = 
";
        let err = mgr
            .parse_hcl_string("bad.hcl", bad_hcl)
            .expect_err("parse error");
        assert!(err.has_errors());

        let bad_json = "{ invalid json }";
        let err_json = mgr
            .parse_json_string("bad.json", bad_json)
            .expect_err("json parse error");
        assert!(err_json.has_errors());
    }

    #[test]
    fn test_file_manager_filesystem_operations() {
        let temp_dir = std::env::temp_dir();
        let hcl_path_buf = temp_dir.join(format!("test_mgr_{}.hcl", std::process::id()));
        let json_path_buf = temp_dir.join(format!("test_mgr_{}.json", std::process::id()));
        let hcl_path = hcl_path_buf.to_str().expect("hcl path str");
        let json_path = json_path_buf.to_str().expect("json path str");

        let mut f_hcl = fs::File::create(hcl_path).expect("create hcl");
        writeln!(f_hcl, "enabled = true").expect("write hcl");
        drop(f_hcl);

        let mut f_json = fs::File::create(json_path).expect("create json");
        writeln!(f_json, r#"{{"count": 5}}"#).expect("write json");
        drop(f_json);

        let mut mgr = FileManager::new();
        let body_hcl = mgr.parse_hcl_file(hcl_path).expect("parse hcl file");
        assert_eq!(body_hcl.attributes.len(), 1);

        let body_json = mgr.parse_json_file(json_path).expect("parse json file");
        assert_eq!(body_json.attributes.len(), 1);

        assert_eq!(mgr.files().len(), 2);

        // Clean up temp files
        let _ = fs::remove_file(hcl_path);
        let _ = fs::remove_file(json_path);

        // Nonexistent file errors
        let non_exist_hcl = mgr.parse_hcl_file("/nonexistent/path/file.hcl");
        assert!(non_exist_hcl.is_err());
        let hcl_err_str = format!("{non_exist_hcl:?}");
        assert!(hcl_err_str.contains("Failed to read file"));

        let non_exist_json = mgr.parse_json_file("/nonexistent/path/file.json");
        assert!(non_exist_json.is_err());
        let json_err_str = format!("{non_exist_json:?}");
        assert!(json_err_str.contains("Failed to read file"));
    }

    #[test]
    fn test_file_manager_buffer_paths() {
        let mut mgr = FileManager::new();
        assert_eq!(mgr.get_buffer_path(1), None);
        mgr.associate_buffer_path(1, "main.tf");
        assert_eq!(mgr.get_buffer_path(1), Some("main.tf"));
        assert_eq!(mgr.get_buffer_path(2), None);
    }
}
