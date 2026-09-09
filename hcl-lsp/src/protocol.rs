//! Language Server Protocol (LSP) 3.17 data types and JSON-RPC definitions.

use serde::{Deserialize, Serialize};

/// Standard JSON-RPC 2.0 error codes.
pub mod error_codes {
    /// Invalid JSON was received by the server.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i64 = -32600;
    /// The method does not exist / is not available.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Invalid method parameter(s).
    pub const INVALID_PARAMS: i64 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// The server received a request before `initialize` was completed.
    pub const SERVER_NOT_INITIALIZED: i64 = -32002;
}

/// A request identifier which may be an integer or a string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// An integer request identifier.
    Number(i64),
    /// A string request identifier.
    String(String),
}

/// A standard JSON-RPC 2.0 request message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// JSON-RPC version string (must be `"2.0"`).
    pub jsonrpc: String,
    /// The request identifier.
    pub id: RequestId,
    /// The method to be invoked.
    pub method: String,
    /// Parameter values for the method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// A standard JSON-RPC 2.0 response message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    /// JSON-RPC version string (must be `"2.0"`).
    pub jsonrpc: String,
    /// The request identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    /// The result on successful execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// The error object in case of failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

impl Response {
    /// Creates a successful response.
    ///
    /// # Arguments
    /// * `id` - The corresponding request identifier.
    /// * `result` - The result value to serialize.
    #[must_use]
    pub fn ok(id: RequestId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            result: Some(result),
            error: None,
        }
    }

    /// Creates an error response.
    ///
    /// # Arguments
    /// * `id` - The optional corresponding request identifier.
    /// * `code` - The error code.
    /// * `message` - The human-readable error description.
    #[must_use]
    pub fn error(id: Option<RequestId>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(ResponseError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// A response error structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseError {
    /// A number indicating the error type that occurred.
    pub code: i64,
    /// A string providing a short description of the error.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// A standard JSON-RPC 2.0 notification message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// JSON-RPC version string (must be `"2.0"`).
    pub jsonrpc: String,
    /// The method to be invoked.
    pub method: String,
    /// Parameter values for the method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl Notification {
    /// Creates a new notification.
    ///
    /// # Arguments
    /// * `method` - The notification method name.
    /// * `params` - Optional notification parameters.
    #[must_use]
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

/// Position in a text document expressed as zero-based line and character offset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    /// Zero-based line position in a document.
    pub line: u32,
    /// Zero-based character offset on a line.
    pub character: u32,
}

impl Position {
    /// Creates a new position.
    ///
    /// # Arguments
    /// * `line` - Zero-based line number.
    /// * `character` - Zero-based character offset.
    #[must_use]
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A range in a text document expressed as start and end positions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    /// The range's start position.
    pub start: Position,
    /// The range's end position.
    pub end: Position,
}

impl Range {
    /// Creates a new range.
    ///
    /// # Arguments
    /// * `start` - The start position.
    /// * `end` - The end position.
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

/// Represents a location inside a resource, such as a line inside a text file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    /// The resource URI.
    pub uri: String,
    /// The range inside the resource.
    pub range: Range,
}

/// An item to transfer a text document from the client to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDocumentItem {
    /// The text document's URI.
    pub uri: String,
    /// The text document's language identifier.
    #[serde(rename = "languageId")]
    pub language_id: String,
    /// The version number of this document.
    pub version: i32,
    /// The content of the opened text document.
    pub text: String,
}

/// Identifier for a text document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    /// The text document's URI.
    pub uri: String,
}

/// An identifier to denote a specific version of a text document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedTextDocumentIdentifier {
    /// The text document's URI.
    pub uri: String,
    /// The version number of this document.
    pub version: i32,
}

/// An event describing a change to a text document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDocumentContentChangeEvent {
    /// The range of the document that changed. If not provided, the entire document was replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// The optional length of the range that got replaced.
    #[serde(
        rename = "rangeLength",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub range_length: Option<u32>,
    /// The new text of the corresponding range or entire document.
    pub text: String,
}

/// Parameters for `textDocument/didOpen` notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DidOpenTextDocumentParams {
    /// The document that was opened.
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentItem,
}

/// Parameters for `textDocument/didChange` notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DidChangeTextDocumentParams {
    /// The document that changed.
    #[serde(rename = "textDocument")]
    pub text_document: VersionedTextDocumentIdentifier,
    /// The actual content changes.
    #[serde(rename = "contentChanges")]
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

/// Parameters for `textDocument/didClose` notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DidCloseTextDocumentParams {
    /// The document that was closed.
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

/// Parameters for `initialize` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitializeParams {
    /// The process ID of the parent process that started the server.
    #[serde(rename = "processId", default)]
    pub process_id: Option<i64>,
    /// The root URI of the workspace.
    #[serde(rename = "rootUri", default)]
    pub root_uri: Option<String>,
    /// The capabilities provided by the client (editor).
    #[serde(default)]
    pub capabilities: serde_json::Value,
}

/// Completion options advertised in server capabilities.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionOptions {
    /// Whether resolving additional completion items is supported.
    #[serde(rename = "resolveProvider", default)]
    pub resolve_provider: Option<bool>,
    /// Trigger characters for completions.
    #[serde(
        rename = "triggerCharacters",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger_characters: Option<Vec<String>>,
}

/// Semantic tokens options advertised in server capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticTokensOptions {
    /// The legend used by the server.
    pub legend: SemanticTokensLegend,
    /// Whether full document semantic token generation is supported.
    #[serde(default)]
    pub full: Option<bool>,
    /// Whether range semantic token generation is supported.
    #[serde(default)]
    pub range: Option<bool>,
}

/// Server capabilities returned in response to the `initialize` request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Defines how text documents are synced. 1 = Full, 2 = Incremental.
    #[serde(rename = "textDocumentSync", default)]
    pub text_document_sync: Option<u32>,
    /// The server provides document symbol support.
    #[serde(rename = "documentSymbolProvider", default)]
    pub document_symbol_provider: Option<bool>,
    /// The server provides hover support.
    #[serde(rename = "hoverProvider", default)]
    pub hover_provider: Option<bool>,
    /// The server provides completion support.
    #[serde(
        rename = "completionProvider",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub completion_provider: Option<CompletionOptions>,
    /// The server provides semantic tokens support.
    #[serde(
        rename = "semanticTokensProvider",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub semantic_tokens_provider: Option<SemanticTokensOptions>,
    /// The server provides goto definition support.
    #[serde(rename = "definitionProvider", default)]
    pub definition_provider: Option<bool>,
    /// The server provides references support.
    #[serde(rename = "referencesProvider", default)]
    pub references_provider: Option<bool>,
}

/// Result returned from `initialize` request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitializeResult {
    /// The capabilities the language server provides.
    pub capabilities: ServerCapabilities,
}

/// Represents programming constructs like variables, blocks, attributes, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSymbol {
    /// The name of this symbol.
    pub name: String,
    /// More detail for this symbol, e.g. the signature or type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The kind of this symbol (see [`SymbolKind`]).
    pub kind: u32,
    /// The range enclosing this symbol not including leading/trailing whitespace.
    pub range: Range,
    /// The range that should be selected and revealed when this symbol is picked.
    #[serde(rename = "selectionRange")]
    pub selection_range: Range,
    /// Children of this symbol, e.g. attributes or sub-blocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DocumentSymbol>>,
}

/// Standard symbol kinds matching LSP specification.
pub struct SymbolKind;

impl SymbolKind {
    /// A namespace symbol (used for blocks).
    pub const NAMESPACE: u32 = 3;
    /// A class symbol.
    pub const CLASS: u32 = 5;
    /// A property symbol (used for block attributes).
    pub const PROPERTY: u32 = 7;
    /// A field symbol.
    pub const FIELD: u32 = 8;
    /// A function symbol (used for functions).
    pub const FUNCTION: u32 = 12;
    /// A variable symbol (used for variables and locals).
    pub const VARIABLE: u32 = 13;
    /// A string symbol (used for block labels).
    pub const STRING: u32 = 15;
}

/// Content formatted with markup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkupContent {
    /// The type of the Markup (e.g. `"markdown"`).
    pub kind: String,
    /// The content itself.
    pub value: String,
}

impl MarkupContent {
    /// Creates a new Markdown markup content object.
    ///
    /// # Arguments
    /// * `value` - The Markdown text string.
    #[must_use]
    pub fn markdown(value: impl Into<String>) -> Self {
        Self {
            kind: "markdown".to_string(),
            value: value.into(),
        }
    }
}

/// The result of a hover request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hover {
    /// The hover's content.
    pub contents: MarkupContent,
    /// An optional range to visualize the hover subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

/// A completion item.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionItem {
    /// The label of this completion item.
    pub label: String,
    /// The kind of this completion item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<u32>,
    /// A human-readable string with additional information about this item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// A human-readable string that represents a doc-comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<MarkupContent>,
    /// A string that should be inserted into a document when selecting this completion.
    #[serde(
        rename = "insertText",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub insert_text: Option<String>,
}

/// Standard completion item kinds.
pub struct CompletionItemKind;

impl CompletionItemKind {
    /// Text completion.
    pub const TEXT: u32 = 1;
    /// Method completion.
    pub const METHOD: u32 = 2;
    /// Function completion.
    pub const FUNCTION: u32 = 3;
    /// Field completion.
    pub const FIELD: u32 = 5;
    /// Variable completion.
    pub const VARIABLE: u32 = 6;
    /// Class / block completion.
    pub const CLASS: u32 = 7;
    /// Property completion.
    pub const PROPERTY: u32 = 10;
    /// Keyword completion.
    pub const KEYWORD: u32 = 14;
    /// Snippet completion.
    pub const SNIPPET: u32 = 15;
}

/// Represents a collection of completion items to be presented in the editor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionList {
    /// This list it not complete. Further typing should result in recomputing this list.
    #[serde(rename = "isIncomplete")]
    pub is_incomplete: bool,
    /// The completion items.
    pub items: Vec<CompletionItem>,
}

/// The legend used to decode semantic tokens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticTokensLegend {
    /// The token types a server uses.
    #[serde(rename = "tokenTypes")]
    pub token_types: Vec<String>,
    /// The token modifiers a server uses.
    #[serde(rename = "tokenModifiers")]
    pub token_modifiers: Vec<String>,
}

/// The result of a `textDocument/semanticTokens/full` or range request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticTokens {
    /// An optional result id.
    #[serde(rename = "resultId", default, skip_serializing_if = "Option::is_none")]
    pub result_id: Option<String>,
    /// The actual tokens encoded in delta format: [deltaLine, deltaStart, length, tokenType, tokenModifiers, ...]
    pub data: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_serialization_roundtrip() {
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(1),
            method: "initialize".to_string(),
            params: None,
        };
        let s = serde_json::to_string(&req).expect("serialize ok");
        assert!(s.contains("\"method\":\"initialize\""));

        let resp = Response::ok(
            RequestId::Number(1),
            serde_json::json!({"capabilities": {}}),
        );
        assert!(resp.result.is_some());

        let err_resp = Response::error(
            Some(RequestId::Number(1)),
            error_codes::METHOD_NOT_FOUND,
            "not found",
        );
        assert!(err_resp.error.is_some());

        let notif = Notification::new("initialized", None);
        assert_eq!(notif.method, "initialized");

        let pos = Position::new(5, 10);
        assert_eq!(pos.line, 5);
        assert_eq!(pos.character, 10);

        let range = Range::new(pos, Position::new(5, 20));
        assert_eq!(range.end.character, 20);

        let loc = Location {
            uri: "file:///test.hcl".to_string(),
            range,
        };
        assert_eq!(loc.uri, "file:///test.hcl");

        let markup = MarkupContent::markdown("# Heading");
        assert_eq!(markup.kind, "markdown");
    }
}
