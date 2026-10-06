//! AST Body and Multi-File Merging Engine.
//!
//! Provides utilities for merging top-level [`crate::ast::structure::Body`] instances, multiple named files,
//! or entire directories of HCL files into a unified AST body while preserving source spans,
//! enforcing deterministic load orders, and checking for attribute/singleton collisions.
use crate::ast::structure::Body;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::span::Span;
use std::collections::HashMap;
use std::path::Path;
/// Assigns an evaluation priority tier to standard HCL block types.
///
/// Priority 1: Core/Root configuration (`packer`, `terraform`)
/// Priority 2: `variable` declarations
/// Priority 3: `locals` declarations
/// Priority 4: `provider`, `plugins`
/// Priority 5: `data` sources
/// Priority 6: `resource`, `source` definitions
/// Priority 7: `build`, `provisioner` blocks
/// Priority 8: All other blocks (retaining lexical file order)
///
/// # Arguments
/// * `block_type` - The identifier of the block type.
#[must_use]
pub fn block_priority(block_type: &str) -> u32 {
    match block_type {
        "packer" | "terraform" => 1,
        "variable" => 2,
        "locals" => 3,
        "provider" | "plugins" => 4,
        "data" => 5,
        "resource" | "source" => 6,
        "build" | "provisioner" => 7,
        _ => 8,
    }
}
/// Configuration options for merging multiple AST bodies and files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeOptions {
    /// List of file extensions to include when merging directories.
    pub extensions: Vec<String>,
    /// Block types that must be unique singletons across all merged bodies.
    pub singleton_blocks: Vec<String>,
    /// Whether to reorder blocks by [`block_priority`].
    pub enforce_priority: bool,
}
impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            extensions: vec!["pkr.hcl".to_string(), "hcl".to_string(), "tf".to_string()],
            singleton_blocks: vec!["packer".to_string(), "terraform".to_string()],
            enforce_priority: true,
        }
    }
}
impl MergeOptions {
    /// Creates a new `MergeOptions` with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Creates unconstrained `MergeOptions` with no singletons and priority sorting disabled.
    #[must_use]
    pub fn unconstrained() -> Self {
        Self {
            extensions: Vec::new(),
            singleton_blocks: Vec::new(),
            enforce_priority: false,
        }
    }
    /// Sets the file extensions to match when scanning directories.
    ///
    /// # Arguments
    /// * `extensions` - An iterator over extension strings.
    #[must_use]
    pub fn with_extensions<I, S>(mut self, extensions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extensions = extensions.into_iter().map(Into::into).collect();
        self
    }
    /// Sets the singleton block types that cannot appear multiple times.
    ///
    /// # Arguments
    /// * `singletons` - An iterator over block type identifiers.
    #[must_use]
    pub fn with_singleton_blocks<I, S>(mut self, singletons: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.singleton_blocks = singletons.into_iter().map(Into::into).collect();
        self
    }
    /// Configures whether block priority ordering should be enforced.
    ///
    /// # Arguments
    /// * `enforce` - If true, blocks are sorted by [`block_priority`].
    #[must_use]
    pub fn with_enforce_priority(mut self, enforce: bool) -> Self {
        self.enforce_priority = enforce;
        self
    }
}
/// Merges multiple `Body` structs into a single unified `Body` using default options.
///
/// The order of the `bodies` slice determines the load order.
///
/// Returns an error if an attribute is defined multiple times at the same level.
///
/// # Arguments
/// * `bodies` - The collection of parsed bodies to merge.
///
/// # Errors
/// Returns [`Diagnostics`] if merging fails due to duplicated attributes.
pub fn merge_bodies(bodies: Vec<Body>) -> Result<Body, Diagnostics> {
    merge_bodies_with_options(bodies, &MergeOptions::unconstrained())
}
/// Merges multiple `Body` structs into a single unified `Body` with custom [`MergeOptions`].
///
/// Returns an error if an attribute is defined multiple times or if any singleton
/// block constraint is violated.
///
/// # Arguments
/// * `bodies` - The collection of parsed bodies to merge.
/// * `options` - Merge configuration options.
///
/// # Errors
/// Returns [`Diagnostics`] if duplicate attributes or duplicate singleton blocks are encountered.
pub fn merge_bodies_with_options(
    bodies: Vec<Body>,
    options: &MergeOptions,
) -> Result<Body, Diagnostics> {
    let mut merged = Body::new(Span::new(0, 0, 0, 0, 0, 0));
    let mut diagnostics = Diagnostics::new();
    let mut seen_singletons: HashMap<String, Span> = HashMap::new();
    for body in bodies {
        merged.span = merged.span.merge(&body.span);
        for (name, attr) in body.attributes {
            if let Some(existing) = merged.attributes.get(&name) {
                let mut diag = Diagnostic::error(
                    format!("Attribute '{name}' defined multiple times"),
                    "Attributes can only be defined once per block or file level",
                    attr.span,
                );
                diag.context = Some(existing.span.clone());
                diagnostics.push(diag);
            } else {
                merged.attributes.insert(name, attr);
            }
        }
        for block in body.blocks {
            if options.singleton_blocks.contains(&block.block_type) {
                if let Some(existing_span) = seen_singletons.get(&block.block_type) {
                    let mut diag = Diagnostic::error(
                        format!(
                            "Singleton block '{}' defined multiple times",
                            block.block_type
                        ),
                        format!(
                            "Only one '{}' block is permitted across configuration files",
                            block.block_type
                        ),
                        block.span.clone(),
                    );
                    diag.context = Some(existing_span.clone());
                    diagnostics.push(diag);
                } else {
                    seen_singletons.insert(block.block_type.clone(), block.span.clone());
                    merged.blocks.push(block);
                }
            } else {
                merged.blocks.push(block);
            }
        }
        merged.functions.extend(body.functions);
        merged.dynamic_blocks.extend(body.dynamic_blocks);
        merged.validations.extend(body.validations);
        merged.preconditions.extend(body.preconditions);
        merged.postconditions.extend(body.postconditions);
    }
    if options.enforce_priority {
        merged.blocks.sort_by_key(|b| block_priority(&b.block_type));
    }
    if diagnostics.has_errors() {
        Err(diagnostics)
    } else {
        Ok(merged)
    }
}
/// Parses and merges multiple named configuration files into a single unified [`Body`].
///
/// Each tuple in `files` contains `(filename, content)`.
///
/// All AST nodes retain their original source file references in their spans.
///
/// # Arguments
/// * `files` - Slice of `(filename, content)` tuples.
///
/// # Errors
/// Returns [`Diagnostics`] if any file fails parsing or if duplicate attribute definitions collide.
pub fn merge_files(files: &[(&str, &str)]) -> Result<Body, Diagnostics> {
    let mut parser = crate::parse::file_manager::FileManager::new();
    let mut diags = Diagnostics::new();
    for (filename, content) in files {
        if let Err(d) = parser.parse_hcl_string(filename, content) {
            for diag in d.errors() {
                diags.push(diag.clone());
            }
        }
    }
    if diags.has_errors() {
        return Err(diags);
    }
    parser.merge_all()
}
/// Discovers and merges all matching configuration files within a directory using default options.
///
/// Files matching any of `extensions` are parsed in lexicographical filename order
/// and merged into a unified [`Body`].
///
/// # Arguments
/// * `path` - The directory path containing configuration files.
/// * `extensions` - Suffix or extension patterns to match (e.g. `&["pkr.hcl", "hcl"]`).
///
/// # Errors
/// Returns [`Diagnostics`] if directory traversal fails, any file fails parsing, or collisions occur.
pub fn merge_directory<P: AsRef<Path>>(path: P, extensions: &[&str]) -> Result<Body, Diagnostics> {
    let opts = MergeOptions::default().with_extensions(extensions.iter().copied());
    merge_directory_with_options(path, &opts)
}
/// Discovers and merges all matching configuration files within a directory using custom [`MergeOptions`].
///
/// # Arguments
/// * `path` - The directory path containing configuration files.
/// * `options` - Custom merge options controlling extensions, singletons, and priority.
///
/// # Errors
/// Returns [`Diagnostics`] if directory traversal fails, any file fails parsing, or collisions occur.
pub fn merge_directory_with_options<P: AsRef<Path>>(
    path: P,
    options: &MergeOptions,
) -> Result<Body, Diagnostics> {
    merge_directory_with_options_impl(path.as_ref(), options)
}
/// Internal non-generic implementation of directory merge operating on a borrowed path.
fn merge_directory_with_options_impl(
    dir_path: &Path,
    options: &MergeOptions,
) -> Result<Body, Diagnostics> {
    let entries = match std::fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(err) => {
            let mut diags = Diagnostics::new();
            diags.push(
                Diagnostic::error(
                    "Failed to read directory",
                    format!("Could not read directory '{}': {err}", dir_path.display()),
                    Span::new(0, 0, 1, 1, 1, 1),
                )
                .with_error(crate::error::HclError::Io(err.to_string())),
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
        let matches_ext = options.extensions.is_empty()
            || options.extensions.iter().any(|ext| {
                let clean_ext = ext.trim_start_matches('.');
                name_str.ends_with(ext.as_str())
                    || (!ext.starts_with('.') && name_str.ends_with(&format!(".{clean_ext}")))
            });
        if matches_ext {
            file_paths.push(entry.path());
        }
    }
    file_paths.sort();
    let mut bodies = Vec::with_capacity(file_paths.len());
    let mut diags = Diagnostics::new();
    for file_path in file_paths {
        let display_name = file_path.to_string_lossy().to_string();
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(err) => {
                diags.push(
                    Diagnostic::error(
                        "Failed to read file",
                        format!("Could not read file '{display_name}': {err}"),
                        Span::new(0, 0, 1, 1, 1, 1),
                    )
                    .with_error(crate::error::HclError::Io(err.to_string())),
                );
                continue;
            }
        };
        let file_arc = Some(std::sync::Arc::from(display_name.as_str()));
        let mut parser = crate::parse::parser::Parser::new_with_file(&content, file_arc);
        let body = parser.parse_body();
        if parser.errors().has_errors() {
            for err in parser.errors().errors() {
                diags.push(err.clone());
            }
        } else {
            bodies.push(body);
        }
    }
    if diags.has_errors() {
        return Err(diags);
    }
    merge_bodies_with_options(bodies, options)
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
    use crate::ast::expr::Expression;
    use crate::ast::structure::{Attribute, Block};
    fn empty_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }
    #[test]
    fn test_merge_bodies_success() {
        let mut b1 = Body::new(empty_span());
        b1.attributes.insert(
            "foo".to_string(),
            Attribute {
                name: "foo".to_string(),
                expr: Expression::Null(empty_span()),
                span: empty_span(),
                name_span: empty_span(),
                equals_span: empty_span(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let mut b2 = Body::new(empty_span());
        b2.attributes.insert(
            "bar".to_string(),
            Attribute {
                name: "bar".to_string(),
                expr: Expression::Null(empty_span()),
                span: empty_span(),
                name_span: empty_span(),
                equals_span: empty_span(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        b2.blocks.push(Block {
            block_type: "my_block".to_string(),
            labels: vec![],
            body: Body::new(empty_span()),
            span: empty_span(),
            type_span: empty_span(),
            label_spans: vec![],
            open_brace_span: empty_span(),
            close_brace_span: empty_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        });
        let merged = merge_bodies(vec![b1, b2]).unwrap();
        assert_eq!(merged.attributes.len(), 2);
        assert_eq!(merged.blocks.len(), 1);
        assert!(merged.attributes.contains_key("foo"));
        assert!(merged.attributes.contains_key("bar"));
    }
    #[test]
    fn test_merge_bodies_duplicate_attribute() {
        let mut b1 = Body::new(empty_span());
        b1.attributes.insert(
            "foo".to_string(),
            Attribute {
                name: "foo".to_string(),
                expr: Expression::Null(empty_span()),
                span: empty_span(),
                name_span: empty_span(),
                equals_span: empty_span(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let mut b2 = Body::new(empty_span());
        b2.attributes.insert(
            "foo".to_string(),
            Attribute {
                name: "foo".to_string(),
                expr: Expression::Bool(true, empty_span()),
                span: empty_span(),
                name_span: empty_span(),
                equals_span: empty_span(),
                leading_comments: Vec::new(),
                trailing_comment: None,
            },
        );
        let errs = merge_bodies(vec![b1, b2]).err().unwrap();
        assert_eq!(errs.errors().len(), 1);
        assert!(
            errs.errors()[0]
                .error
                .to_string()
                .contains("defined multiple times")
        );
    }
    #[test]
    fn test_merge_bodies_validations_and_assertions() {
        use crate::ast::structure::{PostconditionBlock, PreconditionBlock, ValidationBlock};
        let mut b1 = Body::new(empty_span());
        b1.validations.push(ValidationBlock::new(
            Expression::Null(empty_span()),
            Expression::Null(empty_span()),
            empty_span(),
        ));
        b1.preconditions.push(PreconditionBlock::new(
            Expression::Null(empty_span()),
            Expression::Null(empty_span()),
            empty_span(),
        ));
        let mut b2 = Body::new(empty_span());
        b2.postconditions.push(PostconditionBlock::new(
            Expression::Null(empty_span()),
            Expression::Null(empty_span()),
            empty_span(),
        ));
        let merged = merge_bodies(vec![b1, b2]).unwrap();
        assert_eq!(merged.validations.len(), 1);
        assert_eq!(merged.preconditions.len(), 1);
        assert_eq!(merged.postconditions.len(), 1);
    }
    #[test]
    fn test_merge_files_success_and_errors() {
        let f1 = (
            "a.hcl",
            "attr1 = \"val1\"\nservice \"web\" {\n  port = 80\n}\n",
        );
        let f2 = ("b.hcl", "attr2 = 42\nservice \"api\" {\n  port = 8080\n}\n");
        let merged = merge_files(&[f1, f2]).unwrap();
        assert_eq!(merged.attributes.len(), 2);
        assert_eq!(merged.blocks.len(), 2);
        let attr1 = merged.attributes.get("attr1").unwrap();
        assert_eq!(attr1.span.file.as_deref(), Some("a.hcl"));
        let attr2 = merged.attributes.get("attr2").unwrap();
        assert_eq!(attr2.span.file.as_deref(), Some("b.hcl"));
        let f_conflict1 = (
            "conf1.hcl",
            "x = 1
",
        );
        let f_conflict2 = (
            "conf2.hcl",
            "x = 2
",
        );
        let errs = merge_files(&[f_conflict1, f_conflict2]).err().unwrap();
        assert_eq!(errs.errors().len(), 1);
        let diag = &errs.errors()[0];
        assert_eq!(diag.subject.file.as_deref(), Some("conf2.hcl"));
        assert_eq!(
            diag.context.as_ref().and_then(|c| c.file.as_deref()),
            Some("conf1.hcl")
        );
        let f_bad = (
            "bad.hcl",
            "invalid = =
",
        );
        let bad_errs = merge_files(&[f1, f_bad]).err().unwrap();
        assert!(bad_errs.has_errors());
    }
    #[test]
    fn test_block_priority_ordering() {
        assert_eq!(block_priority("packer"), 1);
        assert_eq!(block_priority("terraform"), 1);
        assert_eq!(block_priority("variable"), 2);
        assert_eq!(block_priority("locals"), 3);
        assert_eq!(block_priority("provider"), 4);
        assert_eq!(block_priority("plugins"), 4);
        assert_eq!(block_priority("data"), 5);
        assert_eq!(block_priority("resource"), 6);
        assert_eq!(block_priority("source"), 6);
        assert_eq!(block_priority("build"), 7);
        assert_eq!(block_priority("provisioner"), 7);
        assert_eq!(block_priority("custom"), 8);
    }
    #[test]
    fn test_merge_options_builder() {
        let opts = MergeOptions::new()
            .with_extensions(["hcl", "pkr.hcl"])
            .with_singleton_blocks(["packer", "custom_singleton"])
            .with_enforce_priority(true);
        assert_eq!(opts.extensions, vec!["hcl", "pkr.hcl"]);
        assert_eq!(opts.singleton_blocks, vec!["packer", "custom_singleton"]);
        assert!(opts.enforce_priority);
        let unconstrained = MergeOptions::unconstrained();
        assert_eq!(unconstrained.extensions, Vec::<String>::new());
        assert_eq!(unconstrained.singleton_blocks, Vec::<String>::new());
        assert!(!unconstrained.enforce_priority);
    }
    #[test]
    fn test_merge_bodies_with_options_singletons_and_priority() {
        let make_block = |b_type: &str| Block {
            block_type: b_type.to_string(),
            labels: vec![],
            body: Body::new(empty_span()),
            span: empty_span(),
            type_span: empty_span(),
            label_spans: vec![],
            open_brace_span: empty_span(),
            close_brace_span: empty_span(),
            leading_comments: Vec::new(),
            trailing_comment: None,
        };
        let mut b1 = Body::new(empty_span());
        b1.blocks.push(make_block("build"));
        b1.blocks.push(make_block("packer"));
        let mut b2 = Body::new(empty_span());
        b2.blocks.push(make_block("variable"));
        b2.blocks.push(make_block("locals"));
        let opts = MergeOptions::default();
        let merged = merge_bodies_with_options(vec![b1, b2], &opts).unwrap();
        let types: Vec<&str> = merged
            .blocks
            .iter()
            .map(|b| b.block_type.as_str())
            .collect();
        assert_eq!(types, vec!["packer", "variable", "locals", "build"]);
        let mut b3 = Body::new(empty_span());
        b3.blocks.push(make_block("packer"));
        let mut b4 = Body::new(empty_span());
        b4.blocks.push(make_block("packer"));
        let err = merge_bodies_with_options(vec![b3, b4], &opts)
            .err()
            .unwrap();
        assert_eq!(err.errors().len(), 1);
        assert!(
            err.errors()[0]
                .error
                .to_string()
                .contains("Singleton block 'packer'")
        );
    }
    #[test]
    fn test_merge_directory_success_and_errors() {
        let dir_path = std::env::temp_dir().join(format!("test_merge_dir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir_path);
        std::fs::create_dir_all(&dir_path).unwrap();
        let sub_dir = dir_path.join("sub_dir");
        std::fs::create_dir(&sub_dir).unwrap();
        let f1 = dir_path.join("pkr-variables.pkr.hcl");
        std::fs::write(&f1, "variable \"foo\" {\n  default = \"bar\"\n}\n").unwrap();
        let f2 = dir_path.join("pkr-builder.pkr.hcl");
        std::fs::write(&f2, "build {\n  sources = [\"source.qemu.vm\"]\n}\n").unwrap();
        let f3 = dir_path.join("ignored.txt");
        std::fs::write(&f3, "attr_txt = 1\n").unwrap();
        let hidden = dir_path.join(".hidden.hcl");
        std::fs::write(&hidden, "should be ignored").unwrap();
        let merged = merge_directory(&dir_path, &["pkr.hcl"]).unwrap();
        assert_eq!(merged.blocks.len(), 2);
        assert_eq!(merged.blocks[0].block_type, "variable");
        assert_eq!(merged.blocks[1].block_type, "build");
        assert!(merged.blocks[0].span.file.is_some());
        let merged_dot = merge_directory(&dir_path, &[".pkr.hcl"]).unwrap();
        assert_eq!(merged_dot.blocks.len(), 2);
        let merged_all = merge_directory(&dir_path, &[]).unwrap();
        assert_eq!(merged_all.blocks.len(), 2);
        assert!(merged_all.attributes.contains_key("attr_txt"));
        let bad_path = dir_path.join("nonexistent_sub_dir");
        let err_dir = merge_directory(bad_path, &["hcl"]).err().unwrap();
        assert!(err_dir.has_errors());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let unreadable = dir_path.join("unreadable.pkr.hcl");
            std::fs::write(&unreadable, "x = 1\n").unwrap();
            std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
            let err_read = merge_directory(&dir_path, &["pkr.hcl"]).err().unwrap();
            assert!(err_read.has_errors());
            std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
            let _ = std::fs::remove_file(&unreadable);
        }
        let bad_file = dir_path.join("bad.pkr.hcl");
        std::fs::write(&bad_file, "invalid = = syntax").unwrap();
        let err_parse = merge_directory(&dir_path, &["pkr.hcl"]).err().unwrap();
        assert!(err_parse.has_errors());
        let _ = std::fs::remove_dir_all(&dir_path);
    }
}
