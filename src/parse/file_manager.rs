//! Multi-file parser cache and file manager.
//!
//! Provides [`HclParser`] (and its alias [`FileManager`]) for loading, caching,
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

    /// Discovers, parses, and caches all matching configuration files in a directory.
    ///
    /// # Arguments
    /// * `path` - The directory path containing configuration files.
    /// * `extensions` - Suffix or extension patterns to match (e.g. `&["pkr.hcl", "hcl"]`).
    ///
    /// # Errors
    /// Returns [`Diagnostics`] if directory reading or file parsing fails.
    pub fn parse_directory<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
        extensions: &[&str],
    ) -> Result<Body, Diagnostics> {
        self.parse_directory_impl(path.as_ref(), extensions)
    }

    /// Internal non-generic implementation of directory parsing operating on a borrowed path.
    fn parse_directory_impl(
        &mut self,
        dir_path: &std::path::Path,
        extensions: &[&str],
    ) -> Result<Body, Diagnostics> {
        let entries = match fs::read_dir(dir_path) {
            Ok(e) => e,
            Err(err) => {
                let mut diags = Diagnostics::new();
                diags.push(
                    Diagnostic::error(
                        "Failed to read directory",
                        format!("Could not read directory '{}': {err}", dir_path.display()),
                        Span::new(0, 0, 1, 1, 1, 1),
                    )
                    .with_error(HclError::Io(err.to_string())),
                );
                return Err(diags);
            }
        };

        let mut file_paths = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }

            let matches_ext = extensions.is_empty()
                || extensions.iter().any(|ext| {
                    let clean_ext = ext.trim_start_matches('.');
                    name_str.ends_with(ext)
                        || (!ext.starts_with('.') && name_str.ends_with(&format!(".{clean_ext}")))
                });

            if matches_ext {
                file_paths.push(entry.path());
            }
        }

        file_paths.sort();

        let mut diags = Diagnostics::new();
        for file_path in file_paths {
            let display_name = file_path.to_string_lossy().to_string();
            if let Err(d) = self.parse_hcl_file(&display_name) {
                for diag in d.errors() {
                    diags.push(diag.clone());
                }
            }
        }

        if diags.has_errors() {
            return Err(diags);
        }

        self.merge_all()
    }

    /// Formats all diagnostics across all loaded file buffers using a [`DiagnosticWriter`].
    ///
    /// For each diagnostic, resolves the source buffer matching `diag.subject.file`.
    ///
    /// # Arguments
    /// * `diags` - The diagnostics to format.
    /// * `writer` - The diagnostic writer to use for formatting.
    #[must_use]
    pub fn format_all_diagnostics(&self, diags: &Diagnostics, writer: &DiagnosticWriter) -> String {
        let mut out = String::new();
        for (i, diag) in diags.errors().iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let filename = diag.subject.file.as_deref().unwrap_or("");
            let source = self.get_source(filename).unwrap_or("");
            out.push_str(&writer.format_diagnostic(diag, filename, source));
        }
        out
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
        let body = mgr.parse_hcl_string("vars.hcl", hcl_src).unwrap();
        assert_eq!(body.attributes.len(), 2);
        assert_eq!(mgr.file_count(), 1);
        assert_eq!(mgr.filenames(), vec!["vars.hcl"]);
        assert_eq!(mgr.get_source("vars.hcl"), Some(hcl_src));
        assert!(mgr.get_body("vars.hcl").is_some());
        assert!(mgr.get_file("vars.hcl").is_some());
        assert_eq!(mgr.get_file("nonexistent"), None);

        let json_src = r#"{"baz": 30}"#;
        let json_body = mgr.parse_json_string("extra.json", json_src).unwrap();
        assert_eq!(json_body.attributes.len(), 1);
        assert_eq!(mgr.file_count(), 2);
        assert_eq!(mgr.filenames(), vec!["extra.json", "vars.hcl"]);

        // Merged evaluation
        let merged = mgr.merge_all().unwrap();
        assert_eq!(merged.attributes.len(), 3);

        // Re-parsing existing file (Occupied entry branch)
        let updated_body = mgr.parse_hcl_string("vars.hcl", "foo = 15\n").unwrap();
        assert_eq!(updated_body.attributes.len(), 1);
        let updated_json = mgr
            .parse_json_string("extra.json", r#"{"baz": 35}"#)
            .unwrap();
        assert_eq!(updated_json.attributes.len(), 1);

        let mut ctx = Context::new();
        for (k, attr) in &merged.attributes {
            let (val, _) = Evaluator::new(&ctx).evaluate(&attr.expr).unwrap();
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
        .unwrap();
        mgr.parse_hcl_string(
            "b.hcl",
            "port = 8080
",
        )
        .unwrap();

        let err = mgr.merge_all().err().unwrap();
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
        let err = mgr.parse_hcl_string("bad.hcl", bad_hcl).err().unwrap();
        assert!(err.has_errors());

        let bad_json = "{ invalid json }";
        let err_json = mgr.parse_json_string("bad.json", bad_json).err().unwrap();
        assert!(err_json.has_errors());
    }

    #[test]
    fn test_file_manager_filesystem_operations() {
        let temp_dir = std::env::temp_dir();
        let hcl_path_buf = temp_dir.join(format!("test_mgr_{}.hcl", std::process::id()));
        let json_path_buf = temp_dir.join(format!("test_mgr_{}.json", std::process::id()));
        let hcl_path = hcl_path_buf.to_str().unwrap();
        let json_path = json_path_buf.to_str().unwrap();

        let mut f_hcl = fs::File::create(hcl_path).unwrap();
        writeln!(f_hcl, "enabled = true").unwrap();
        drop(f_hcl);

        let mut f_json = fs::File::create(json_path).unwrap();
        writeln!(f_json, r#"{{"count": 5}}"#).unwrap();
        drop(f_json);

        let mut mgr = FileManager::new();
        let body_hcl = mgr.parse_hcl_file(hcl_path).unwrap();
        assert_eq!(body_hcl.attributes.len(), 1);

        let body_json = mgr.parse_json_file(json_path).unwrap();
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

    #[test]
    fn test_file_manager_parse_directory_and_format_all_diagnostics() {
        let temp_dir = std::env::temp_dir().join(format!("test_fm_dir_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let p1 = temp_dir.join("a.hcl");
        let p2 = temp_dir.join("b.hcl");
        let sub_dir = temp_dir.join("sub_dir");
        let hidden = temp_dir.join(".hidden.hcl");
        let non_matching = temp_dir.join("ignored.txt");

        fs::create_dir(&sub_dir).unwrap();
        fs::write(&p1, "val_a = \"hello\"\n").unwrap();
        fs::write(&p2, "val_b = \"world\"\n").unwrap();
        fs::write(&hidden, "hidden = 1\n").unwrap();
        fs::write(&non_matching, "attr_txt = 1\n").unwrap();

        let mut mgr = FileManager::new();
        let body = mgr.parse_directory(&temp_dir, &["hcl"]).unwrap();
        assert_eq!(body.attributes.len(), 2);
        assert_eq!(mgr.file_count(), 2);

        // Test with leading dot and empty extensions
        let mut mgr_dot = FileManager::new();
        let body_dot = mgr_dot.parse_directory(&temp_dir, &[".hcl"]).unwrap();
        assert_eq!(body_dot.attributes.len(), 2);

        let mut mgr_all = FileManager::new();
        let body_all = mgr_all.parse_directory(&temp_dir, &[]).unwrap();
        assert_eq!(body_all.attributes.len(), 3);

        // Test format_all_diagnostics
        let mut diags = Diagnostics::new();
        let span_a = Span::new(0, 5, 1, 1, 1, 6).with_file(p1.to_str().unwrap());
        diags.push(Diagnostic::error("Test error A", "Detail A", span_a));

        let span_b = Span::new(0, 5, 1, 1, 1, 6).with_file(p2.to_str().unwrap());
        diags.push(Diagnostic::warning("Test warning B", "Detail B", span_b));

        let writer = DiagnosticWriter::plain();
        let formatted = mgr.format_all_diagnostics(&diags, &writer);
        assert!(formatted.contains("Test error A"));
        assert!(formatted.contains("Test warning B"));
        assert!(formatted.contains("val_a = \"hello\""));
        assert!(formatted.contains("val_b = \"world\""));

        // Test parse error in one directory file
        let bad_hcl = temp_dir.join("c_bad.hcl");
        fs::write(&bad_hcl, "bad = = parse error").unwrap();
        let mut mgr_err = FileManager::new();
        let err_parse = mgr_err.parse_directory(&temp_dir, &["hcl"]).err().unwrap();
        assert!(err_parse.has_errors());
        let _ = fs::remove_file(&bad_hcl);

        // Test non-existent directory error
        let bad_dir = temp_dir.join("missing");
        let mut bad_mgr = FileManager::new();
        let err = bad_mgr.parse_directory(bad_dir, &["hcl"]).err().unwrap();
        assert!(err.has_errors());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
