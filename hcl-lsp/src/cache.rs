//! In-memory virtual document cache and synchronization engine.
//!
//! Synchronizes open documents across client edits (full and incremental text sync)
//! and maintains parsed AST [`Body`] and CST [`CstFile`] representations.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::error::HclError;
use crate::protocol::{Position, Range, TextDocumentContentChangeEvent, TextDocumentItem};
use hashicorp_configuration_language_rs::analysis::Linter;
use hashicorp_configuration_language_rs::analysis::type_check::TypeChecker;
use hashicorp_configuration_language_rs::ast::schema::BodySchema;
use hashicorp_configuration_language_rs::ast::structure::Body;
use hashicorp_configuration_language_rs::cst::CstFile;
use hashicorp_configuration_language_rs::diagnostic::Diagnostics;
use hashicorp_configuration_language_rs::parse::parser::Parser;
use std::collections::HashMap;

/// An in-memory open text document with its parsed AST and CST representations.
#[derive(Debug, Clone)]
pub struct VirtualDocument {
    /// The document's URI identifier.
    pub uri: String,
    /// The language identifier (e.g. `"hcl"` or `"terraform"`).
    pub language_id: String,
    /// The document version counter.
    pub version: i32,
    /// The full current text content.
    pub text: String,
    /// The parsed AST body representation.
    pub parsed_body: Option<Body>,
    /// The parsed CST file representation.
    pub cst_file: Option<CstFile>,
    /// Accumulated diagnostics (syntax errors and linter warnings).
    pub diagnostics: Diagnostics,
}

impl VirtualDocument {
    /// Creates a new `VirtualDocument` from an LSP [`TextDocumentItem`] and parses it.
    ///
    /// # Arguments
    /// * `item` - The document item provided by the client.
    /// * `schema` - Optional declarative schema for type checking.
    #[must_use]
    pub fn new(item: TextDocumentItem, schema: Option<&BodySchema>) -> Self {
        let mut doc = Self {
            uri: item.uri,
            language_id: item.language_id,
            version: item.version,
            text: item.text,
            parsed_body: None,
            cst_file: None,
            diagnostics: Diagnostics::new(),
        };
        doc.reparse(schema);
        doc
    }

    /// Applies an incremental or full content change event and reparses the document.
    ///
    /// # Arguments
    /// * `change` - The change event from `textDocument/didChange`.
    /// * `schema` - Optional declarative schema for type checking.
    pub fn apply_change(
        &mut self,
        change: &TextDocumentContentChangeEvent,
        schema: Option<&BodySchema>,
    ) {
        if let Some(range) = change.range {
            self.apply_incremental_change(range, &change.text);
        } else {
            self.text.clone_from(&change.text);
        }
        self.reparse(schema);
    }

    fn apply_incremental_change(&mut self, range: Range, new_text: &str) {
        let start_offset = self.position_to_offset(range.start);
        let end_offset = self.position_to_offset(range.end);
        let safe_start = start_offset.min(self.text.len());
        let safe_end = end_offset.min(self.text.len()).max(safe_start);
        let mut updated = String::with_capacity(self.text.len() + new_text.len());
        updated.push_str(&self.text[..safe_start]);
        updated.push_str(new_text);
        updated.push_str(&self.text[safe_end..]);
        self.text = updated;
    }

    /// Converts a zero-based line and character [`Position`] into a UTF-8 byte offset in the document.
    ///
    /// Safely maps character offsets to valid UTF-8 codepoint boundaries using `char_indices()`,
    /// ensuring multi-byte characters are never sliced mid-codepoint.
    ///
    /// # Arguments
    /// * `pos` - The position to translate.
    ///
    /// # Returns
    /// The UTF-8 byte index in `self.text`.
    #[must_use]
    pub fn position_to_offset(&self, pos: Position) -> usize {
        let newline = char::from(10);
        let cr = char::from(13);
        let mut current_offset = 0usize;
        for (current_line, line) in self.text.split_inclusive(newline).enumerate() {
            if current_line == pos.line as usize {
                let target_chars = pos.character as usize;
                let line_without_newline = line.strip_suffix(newline).unwrap_or(line);
                let line_without_cr = line_without_newline
                    .strip_suffix(cr)
                    .unwrap_or(line_without_newline);
                let byte_in_line = line_without_cr
                    .char_indices()
                    .nth(target_chars)
                    .map_or(line_without_cr.len(), |(idx, _)| idx);
                return current_offset + byte_in_line;
            }
            current_offset += line.len();
        }
        self.text.len()
    }

    /// Converts a byte offset in the document into a zero-based [`Position`].
    ///
    /// # Arguments
    /// * `offset` - The byte index.
    ///
    /// # Returns
    /// The corresponding zero-based line and character offset.
    #[must_use]
    pub fn offset_to_position(&self, offset: usize) -> Position {
        let newline = char::from(10);
        let safe_offset = offset.min(self.text.len());
        let mut line = 0u32;
        let mut character = 0u32;
        for (idx, ch) in self.text.char_indices() {
            if idx >= safe_offset {
                break;
            }
            if ch == newline {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }
        Position::new(line, character)
    }

    /// Reparses the AST and CST from `self.text`, regenerating diagnostics and linter warnings.
    ///
    /// # Arguments
    /// * `schema` - Optional declarative schema for type checking.
    pub fn reparse(&mut self, schema: Option<&BodySchema>) {
        let mut diags = Diagnostics::new();
        let mut parser = Parser::new(&self.text);
        let body = parser.parse_body();
        diags.extend(parser.errors().clone());
        let linter = Linter::new();
        let lint_diags = linter.lint_body(&body);
        diags.extend(lint_diags);
        if let Some(s) = schema {
            let checker = TypeChecker::new();
            if let Err(type_diags) = checker.check_body(&body, s) {
                diags.extend(type_diags);
            }
        }
        self.parsed_body = Some(body);
        self.cst_file = CstFile::parse(&self.text).ok();
        self.diagnostics = diags;
    }
}

/// In-memory cache of all open virtual documents indexed by their URI.
#[derive(Debug, Clone, Default)]
pub struct DocumentCache {
    /// Mapping of document URIs to active virtual documents.
    pub documents: HashMap<String, VirtualDocument>,
}

impl DocumentCache {
    /// Creates a new, empty document cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens and registers a new document.
    ///
    /// # Arguments
    /// * `item` - The document item opened by the client.
    /// * `schema` - Optional declarative schema for type checking.
    ///
    /// # Returns
    /// A reference to the newly cached document.
    pub fn open_document(
        &mut self,
        item: TextDocumentItem,
        schema: Option<&BodySchema>,
    ) -> &VirtualDocument {
        let uri = item.uri.clone();
        let doc = VirtualDocument::new(item, schema);
        self.documents.insert(uri.clone(), doc);
        &self.documents[&uri]
    }

    /// Applies changes to an open document.
    ///
    /// # Arguments
    /// * `uri` - The document URI.
    /// * `version` - The new version number.
    /// * `changes` - The list of change events.
    /// * `schema` - Optional declarative schema for type checking.
    ///
    /// # Errors
    /// Returns [`HclError::LspDocumentNotFound`] if the URI is not in the cache.
    pub fn change_document(
        &mut self,
        uri: &str,
        version: i32,
        changes: &[TextDocumentContentChangeEvent],
        schema: Option<&BodySchema>,
    ) -> Result<&VirtualDocument, HclError> {
        let Some(doc) = self.documents.get_mut(uri) else {
            return Err(HclError::LspDocumentNotFound(uri.to_string()));
        };
        doc.version = version;
        for change in changes {
            doc.apply_change(change, schema);
        }
        Ok(&self.documents[uri])
    }

    /// Closes and removes a document from the cache.
    ///
    /// # Arguments
    /// * `uri` - The document URI to close.
    ///
    /// # Returns
    /// The removed document if it was open.
    pub fn close_document(&mut self, uri: &str) -> Option<VirtualDocument> {
        self.documents.remove(uri)
    }

    /// Retrieves an open document by its URI.
    ///
    /// # Arguments
    /// * `uri` - The document URI.
    ///
    /// # Returns
    /// An optional reference to the document.
    #[must_use]
    pub fn get_document(&self, uri: &str) -> Option<&VirtualDocument> {
        self.documents.get(uri)
    }

    /// Returns the number of open documents in the cache.
    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Returns `true` if the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashicorp_configuration_language_rs::ast::schema::AttributeSchema;
    use hashicorp_configuration_language_rs::types::Type;

    #[test]
    fn test_document_cache_open_change_close() -> Result<(), HclError> {
        let mut cache = DocumentCache::new();
        assert!(cache.is_empty());
        let item = TextDocumentItem {
            uri: "file:///test.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "a = 1
"
            .to_string(),
        };
        let doc = cache.open_document(item, None);
        assert_eq!(doc.version, 1);
        assert_eq!(cache.len(), 1);
        let change_full = TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "a = 2
b = 3
"
            .to_string(),
        };
        let updated = cache.change_document("file:///test.hcl", 2, &[change_full], None)?;
        assert_eq!(updated.version, 2);
        assert_eq!(
            updated.text,
            "a = 2
b = 3
"
        );
        let change_inc = TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 4), Position::new(0, 5))),
            range_length: Some(1),
            text: "42".to_string(),
        };
        let updated_inc = cache.change_document("file:///test.hcl", 3, &[change_inc], None)?;
        assert_eq!(
            updated_inc.text,
            "a = 42
b = 3
"
        );
        let closed = cache.close_document("file:///test.hcl");
        assert!(closed.is_some());
        assert!(cache.is_empty());
        let err = cache.change_document("file:///nonexistent.tf", 2, &[], None);
        assert!(matches!(err, Err(HclError::LspDocumentNotFound(_))));
        Ok(())
    }

    #[test]
    fn test_position_and_offset_conversions_with_multibyte_utf8() -> Result<(), HclError> {
        let text = "line0
line1
line2";
        let item = TextDocumentItem {
            uri: "file:///sample.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: text.to_string(),
        };
        let doc = VirtualDocument::new(item, None);
        let pos = Position::new(1, 2);
        let offset = doc.position_to_offset(pos);
        assert_eq!(offset, 8);
        let roundtrip_pos = doc.offset_to_position(offset);
        assert_eq!(roundtrip_pos.line, 1);
        assert_eq!(roundtrip_pos.character, 2);
        let beyond_offset = doc.position_to_offset(Position::new(99, 0));
        assert_eq!(beyond_offset, doc.text.len());
        let beyond_col = doc.position_to_offset(Position::new(0, 99));
        assert_eq!(beyond_col, 5);

        // Multi-byte UTF-8 incremental edit test (emoji + accented characters + CRLF)
        let mut cache = DocumentCache::new();
        let cr = char::from(13);
        let lf = char::from(10);
        let utf8_item = TextDocumentItem {
            uri: "file:///utf8.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: format!(r#"msg = "🚀é"{cr}{lf}"#),
        };
        cache.open_document(utf8_item, None);
        // Replace 'é' (character index 8 on line 0: m(0) s(1) g(2)  (3) =(4)  (5) "(6) 🚀(7) é(8))
        let change_utf8 = TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 8), Position::new(0, 9))),
            range_length: Some(1),
            text: "✨".to_string(),
        };
        let updated_utf8 = cache.change_document("file:///utf8.hcl", 2, &[change_utf8], None)?;
        assert_eq!(updated_utf8.text, format!(r#"msg = "🚀✨"{cr}{lf}"#));

        // Schema validation branch test
        let schema = BodySchema::new()
            .with_attribute(AttributeSchema::required("req").with_type(Type::Bool));
        let item_schema = TextDocumentItem {
            uri: "file:///schema.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "req = 42\n".to_string(),
        };
        let doc_schema = VirtualDocument::new(item_schema, Some(&schema));
        assert!(doc_schema.diagnostics.has_errors());
        let item_schema_ok = TextDocumentItem {
            uri: "file:///schema_ok.hcl".to_string(),
            language_id: "hcl".to_string(),
            version: 1,
            text: "req = true\n".to_string(),
        };
        let doc_schema_ok = VirtualDocument::new(item_schema_ok, Some(&schema));
        assert!(!doc_schema_ok.diagnostics.has_errors());
        Ok(())
    }
}
