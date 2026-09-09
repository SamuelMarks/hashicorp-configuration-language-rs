//! LSP server daemon and request dispatch engine.
//!
//! Implements document lifecycles, JSON-RPC routing, and diagnostics publishing over stdio and TCP.

use crate::cache::DocumentCache;
use crate::completion::completions_at_position;
use crate::definition::{find_references, goto_definition};
use crate::error::LspError;
use crate::hover::hover_at_position;
use crate::protocol::error_codes;
use crate::protocol::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, Notification, Position, Range, Request, RequestId,
    Response, SemanticTokensOptions, ServerCapabilities,
};
use crate::semantic_tokens::{semantic_tokens_full, semantic_tokens_legend, semantic_tokens_range};
use crate::symbols::document_symbols;
use crate::transport::Transport;
use hashicorp_configuration_language_rs::ast::schema::BodySchema;
use hashicorp_configuration_language_rs::diagnostic::json::DiagnosticJson;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpListener;

/// The central LSP language server daemon.
#[derive(Debug, Default)]
pub struct LspServer {
    /// In-memory document cache synchronizing open buffers.
    pub cache: DocumentCache,
    /// Optional declarative schema for completion, hover, and validation.
    pub schema: Option<BodySchema>,
    /// Whether the server has received and responded to `initialize`.
    pub is_initialized: bool,
    /// Whether the server has received a `shutdown` request.
    pub is_shutdown: bool,
}

impl LspServer {
    /// Creates a new `LspServer`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the server with an optional declarative schema.
    ///
    /// # Arguments
    /// * `schema` - Declarative body schema.
    ///
    /// # Returns
    /// The updated `LspServer`.
    #[must_use]
    pub fn with_schema(mut self, schema: BodySchema) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Dispatches a JSON-RPC request message and generates the appropriate response.
    ///
    /// # Arguments
    /// * `req` - The incoming request message.
    ///
    /// # Returns
    /// The generated [`Response`] message.
    pub fn handle_request(&mut self, req: Request) -> Response {
        if !self.is_initialized && req.method != "initialize" {
            return Response::error(
                Some(req.id),
                error_codes::SERVER_NOT_INITIALIZED,
                "Server is not initialized yet",
            );
        }

        if self.is_shutdown && req.method != "exit" {
            return Response::error(
                Some(req.id),
                error_codes::INVALID_REQUEST,
                "Server has shut down",
            );
        }

        match req.method.as_str() {
            "initialize" => self.handle_initialize(req.id, req.params),
            "shutdown" => {
                self.is_shutdown = true;
                Response::ok(req.id, serde_json::Value::Null)
            }
            "textDocument/documentSymbol" => self.handle_document_symbol(req.id, req.params),
            "textDocument/hover" => self.handle_hover(req.id, req.params),
            "textDocument/completion" => self.handle_completion(req.id, req.params),
            "textDocument/semanticTokens/full" => {
                self.handle_semantic_tokens_full(req.id, req.params)
            }
            "textDocument/semanticTokens/range" => {
                self.handle_semantic_tokens_range(req.id, req.params)
            }
            "textDocument/definition" => self.handle_definition(req.id, req.params),
            "textDocument/references" => self.handle_references(req.id, req.params),
            _ => Response::error(
                Some(req.id),
                error_codes::METHOD_NOT_FOUND,
                format!("Method '{}' not implemented", req.method),
            ),
        }
    }

    /// Handles a JSON-RPC notification message and optionally returns a notification to emit.
    ///
    /// # Arguments
    /// * `notif` - The incoming notification message.
    ///
    /// # Returns
    /// An optional [`Notification`] (e.g. `textDocument/publishDiagnostics`).
    pub fn handle_notification(&mut self, notif: Notification) -> Option<Notification> {
        match notif.method.as_str() {
            "initialized" => {
                self.is_initialized = true;
                None
            }
            "textDocument/didOpen" => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notif.params?).ok()?;
                let doc = self.cache.open_document(params.text_document);
                Some(create_publish_diagnostics_notification(
                    &doc.uri,
                    doc.diagnostics.errors(),
                    Some(&doc.text),
                ))
            }
            "textDocument/didChange" => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notif.params?).ok()?;
                let doc = self
                    .cache
                    .change_document(
                        &params.text_document.uri,
                        params.text_document.version,
                        &params.content_changes,
                    )
                    .ok()?;
                Some(create_publish_diagnostics_notification(
                    &doc.uri,
                    doc.diagnostics.errors(),
                    Some(&doc.text),
                ))
            }
            "textDocument/didClose" => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notif.params?).ok()?;
                self.cache.close_document(&params.text_document.uri);
                None
            }
            _ => None,
        }
    }

    /// Handles the `initialize` lifecycle request.
    fn handle_initialize(&mut self, id: RequestId, params: Option<serde_json::Value>) -> Response {
        let _init_params: Option<InitializeParams> =
            params.and_then(|p| serde_json::from_value(p).ok());
        self.is_initialized = true;

        let capabilities = ServerCapabilities {
            text_document_sync: Some(2), // Incremental
            document_symbol_provider: Some(true),
            hover_provider: Some(true),
            completion_provider: Some(crate::protocol::CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![".".to_string(), "=".to_string(), "\"".to_string()]),
            }),
            semantic_tokens_provider: Some(SemanticTokensOptions {
                legend: semantic_tokens_legend(),
                full: Some(true),
                range: Some(true),
            }),
            definition_provider: Some(true),
            references_provider: Some(true),
        };

        let result = InitializeResult { capabilities };
        Response::ok(
            id,
            serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Handles document outline symbol extraction requests.
    fn handle_document_symbol(&self, id: RequestId, params: Option<serde_json::Value>) -> Response {
        let Some(p) = params else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Missing params");
        };
        let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
        let Some(doc) = self.cache.get_document(uri) else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Document not found");
        };

        let symbols = document_symbols(doc);
        Response::ok(
            id,
            serde_json::to_value(symbols).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Handles hover documentation requests.
    fn handle_hover(&self, id: RequestId, params: Option<serde_json::Value>) -> Response {
        let Some(p) = params else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Missing params");
        };
        let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
        let Some(doc) = self.cache.get_document(uri) else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Document not found");
        };

        let line = p["position"]["line"].as_u64().unwrap_or_default() as u32;
        let char_offset = p["position"]["character"].as_u64().unwrap_or_default() as u32;
        let pos = Position::new(line, char_offset);

        let hover = hover_at_position(doc, pos, self.schema.as_ref());
        Response::ok(
            id,
            serde_json::to_value(hover).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Handles code completion requests.
    fn handle_completion(&self, id: RequestId, params: Option<serde_json::Value>) -> Response {
        let Some(p) = params else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Missing params");
        };
        let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
        let Some(doc) = self.cache.get_document(uri) else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Document not found");
        };

        let line = p["position"]["line"].as_u64().unwrap_or_default() as u32;
        let char_offset = p["position"]["character"].as_u64().unwrap_or_default() as u32;
        let pos = Position::new(line, char_offset);

        let completions = completions_at_position(doc, pos, self.schema.as_ref());
        Response::ok(
            id,
            serde_json::to_value(completions).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Handles full-document semantic tokens extraction requests.
    fn handle_semantic_tokens_full(
        &self,
        id: RequestId,
        params: Option<serde_json::Value>,
    ) -> Response {
        let Some(p) = params else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Missing params");
        };
        let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
        let Some(doc) = self.cache.get_document(uri) else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Document not found");
        };

        let tokens = semantic_tokens_full(doc);
        Response::ok(
            id,
            serde_json::to_value(tokens).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Handles range-scoped semantic tokens extraction requests.
    fn handle_semantic_tokens_range(
        &self,
        id: RequestId,
        params: Option<serde_json::Value>,
    ) -> Response {
        let Some(p) = params else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Missing params");
        };
        let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
        let Some(doc) = self.cache.get_document(uri) else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Document not found");
        };

        let range: Range = match serde_json::from_value(p["range"].clone()) {
            Ok(r) => r,
            Err(_) => {
                return Response::error(Some(id), error_codes::INVALID_PARAMS, "Invalid range");
            }
        };

        let tokens = semantic_tokens_range(doc, range);
        Response::ok(
            id,
            serde_json::to_value(tokens).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Handles goto definition lookup requests.
    fn handle_definition(&self, id: RequestId, params: Option<serde_json::Value>) -> Response {
        let Some(p) = params else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Missing params");
        };
        let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
        let Some(doc) = self.cache.get_document(uri) else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Document not found");
        };

        let line = p["position"]["line"].as_u64().unwrap_or_default() as u32;
        let char_offset = p["position"]["character"].as_u64().unwrap_or_default() as u32;
        let pos = Position::new(line, char_offset);

        let def = goto_definition(doc, pos, &self.cache);
        Response::ok(
            id,
            serde_json::to_value(def).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Handles find references lookup requests.
    fn handle_references(&self, id: RequestId, params: Option<serde_json::Value>) -> Response {
        let Some(p) = params else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Missing params");
        };
        let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
        let Some(doc) = self.cache.get_document(uri) else {
            return Response::error(Some(id), error_codes::INVALID_PARAMS, "Document not found");
        };

        let line = p["position"]["line"].as_u64().unwrap_or_default() as u32;
        let char_offset = p["position"]["character"].as_u64().unwrap_or_default() as u32;
        let pos = Position::new(line, char_offset);

        let refs = find_references(doc, pos, &self.cache);
        Response::ok(
            id,
            serde_json::to_value(refs).unwrap_or(serde_json::Value::Null),
        )
    }

    /// Runs the message loop reading from a standard input stream and writing to output.
    ///
    /// # Arguments
    /// * `input` - The input byte stream.
    /// * `output` - The output byte stream.
    ///
    /// # Errors
    /// Returns [`LspError`] if communication encounters unrecoverable errors.
    pub fn run_stream<R: Read, W: Write>(&mut self, input: R, output: W) -> Result<(), LspError> {
        let mut reader = BufReader::new(input);
        let mut writer = BufWriter::new(output);

        while let Some(msg_str) = Transport::read_message(&mut reader)? {
            if let Ok(req) = serde_json::from_str::<Request>(&msg_str) {
                let resp = self.handle_request(req);
                let resp_str = serde_json::to_string(&resp).unwrap_or_default();
                Transport::write_message(&mut writer, &resp_str)?;
            } else if let Ok(notif) = serde_json::from_str::<Notification>(&msg_str) {
                let is_exit = notif.method == "exit";
                if let Some(out_notif) = self.handle_notification(notif) {
                    let out_str = serde_json::to_string(&out_notif).unwrap_or_default();
                    Transport::write_message(&mut writer, &out_str)?;
                }
                if is_exit {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Listens for a single TCP client connection on `addr` and runs the server loop.
    ///
    /// # Arguments
    /// * `addr` - The TCP bind address (e.g. `"127.0.0.1:9257"`).
    ///
    /// # Errors
    /// Returns [`LspError`] if binding or communicating fails.
    pub fn run_tcp(&mut self, addr: &str) -> Result<(), LspError> {
        let listener = TcpListener::bind(addr)?;
        let (stream, _) = listener.accept()?;
        let reader = stream.try_clone()?;
        self.run_stream(reader, stream)
    }
}

/// Creates a `textDocument/publishDiagnostics` notification for document diagnostics.
fn create_publish_diagnostics_notification(
    uri: &str,
    diags: &[hashicorp_configuration_language_rs::diagnostic::Diagnostic],
    source: Option<&str>,
) -> Notification {
    let diag_jsons: Vec<DiagnosticJson> = diags
        .iter()
        .map(|d| DiagnosticJson::from_diagnostic(d, source))
        .collect();

    let params = serde_json::json!({
        "uri": uri,
        "diagnostics": diag_jsons,
    });

    Notification::new("textDocument/publishDiagnostics", Some(params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_server_initialize_and_document_lifecycle() {
        let mut server = LspServer::new();

        // 1. Initialize
        let init_req = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({})),
        };
        let init_resp = server.handle_request(init_req);
        assert!(init_resp.result.is_some());
        assert!(server.is_initialized);

        // 2. Initialized notification
        let initialized_notif = Notification::new("initialized", None);
        assert!(server.handle_notification(initialized_notif).is_none());

        // 3. didOpen
        let open_notif = Notification::new(
            "textDocument/didOpen",
            Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///main.tf",
                    "languageId": "hcl",
                    "version": 1,
                    "text": "name = \"test\"\n",
                }
            })),
        );
        let pub_diag = server.handle_notification(open_notif);
        assert!(pub_diag.is_some());

        // 4. documentSymbol
        let sym_req = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(2),
            method: "textDocument/documentSymbol".to_string(),
            params: Some(serde_json::json!({
                "textDocument": { "uri": "file:///main.tf" }
            })),
        };
        let sym_resp = server.handle_request(sym_req);
        assert!(sym_resp.result.is_some());

        // 5. Shutdown
        let shut_req = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(3),
            method: "shutdown".to_string(),
            params: None,
        };
        let shut_resp = server.handle_request(shut_req);
        assert!(shut_resp.result.is_some());
        assert!(server.is_shutdown);

        // 6. Exit request after shutdown
        let exit_req = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(4),
            method: "exit".to_string(),
            params: None,
        };
        let exit_resp = server.handle_request(exit_req);
        assert!(exit_resp.error.is_some());

        // 7. Request after shutdown fails
        let post_req = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(5),
            method: "textDocument/hover".to_string(),
            params: None,
        };
        let post_resp = server.handle_request(post_req);
        assert!(post_resp.error.is_some());
    }

    #[test]
    fn test_server_all_requests_and_errors() {
        let mut server = LspServer::new().with_schema(BodySchema::new());

        // Pre-initialization error
        let pre_req = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(1),
            method: "textDocument/hover".to_string(),
            params: None,
        };
        let pre_resp = server.handle_request(pre_req);
        assert_eq!(
            pre_resp.error.as_ref().map(|e| e.code),
            Some(error_codes::SERVER_NOT_INITIALIZED)
        );

        // Initialize
        let init_req = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(2),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({})),
        };
        assert!(server.handle_request(init_req).result.is_some());

        // Open a document
        let open_notif = Notification::new(
            "textDocument/didOpen",
            Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///test.hcl",
                    "languageId": "hcl",
                    "version": 1,
                    "text": "foo = 1\n",
                }
            })),
        );
        assert!(server.handle_notification(open_notif).is_some());

        let methods = [
            "textDocument/documentSymbol",
            "textDocument/hover",
            "textDocument/completion",
            "textDocument/semanticTokens/full",
            "textDocument/definition",
            "textDocument/references",
        ];

        for (i, &method) in methods.iter().enumerate() {
            let id = RequestId::Number(10 + i as i64);
            // 1. Missing params
            let req_no_params = Request {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                method: method.to_string(),
                params: None,
            };
            let resp_no_params = server.handle_request(req_no_params);
            assert_eq!(
                resp_no_params.error.as_ref().map(|e| e.code),
                Some(error_codes::INVALID_PARAMS)
            );

            // 2. Missing document
            let req_missing_doc = Request {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                method: method.to_string(),
                params: Some(serde_json::json!({
                    "textDocument": { "uri": "file:///nonexistent.hcl" },
                    "position": { "line": 0, "character": 0 }
                })),
            };
            let resp_missing_doc = server.handle_request(req_missing_doc);
            assert_eq!(
                resp_missing_doc.error.as_ref().map(|e| e.code),
                Some(error_codes::INVALID_PARAMS)
            );

            // 3. Valid document
            let req_valid = Request {
                jsonrpc: "2.0".to_string(),
                id,
                method: method.to_string(),
                params: Some(serde_json::json!({
                    "textDocument": { "uri": "file:///test.hcl" },
                    "position": { "line": 0, "character": 0 }
                })),
            };
            let resp_valid = server.handle_request(req_valid);
            assert!(resp_valid.result.is_some());
        }

        // semanticTokens/range special cases
        let range_method = "textDocument/semanticTokens/range";
        // 1. Missing params
        let req_no_params = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(50),
            method: range_method.to_string(),
            params: None,
        };
        assert_eq!(
            server
                .handle_request(req_no_params)
                .error
                .as_ref()
                .map(|e| e.code),
            Some(error_codes::INVALID_PARAMS)
        );

        // 2. Missing doc
        let req_miss_doc = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(51),
            method: range_method.to_string(),
            params: Some(serde_json::json!({
                "textDocument": { "uri": "file:///nonexistent.hcl" }
            })),
        };
        assert_eq!(
            server
                .handle_request(req_miss_doc)
                .error
                .as_ref()
                .map(|e| e.code),
            Some(error_codes::INVALID_PARAMS)
        );

        // 3. Invalid range
        let req_bad_range = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(52),
            method: range_method.to_string(),
            params: Some(serde_json::json!({
                "textDocument": { "uri": "file:///test.hcl" },
                "range": "invalid-range"
            })),
        };
        assert_eq!(
            server
                .handle_request(req_bad_range)
                .error
                .as_ref()
                .map(|e| e.code),
            Some(error_codes::INVALID_PARAMS)
        );

        // 4. Valid range
        let req_good_range = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(53),
            method: range_method.to_string(),
            params: Some(serde_json::json!({
                "textDocument": { "uri": "file:///test.hcl" },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 5 }
                }
            })),
        };
        assert!(server.handle_request(req_good_range).result.is_some());

        // Unknown method
        let req_unknown = Request {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(60),
            method: "custom/unknown".to_string(),
            params: None,
        };
        assert_eq!(
            server
                .handle_request(req_unknown)
                .error
                .as_ref()
                .map(|e| e.code),
            Some(error_codes::METHOD_NOT_FOUND)
        );
    }

    #[test]
    fn test_server_notifications() {
        let mut server = LspServer::new();

        // 1. didOpen with invalid params
        let bad_open = Notification::new("textDocument/didOpen", Some(serde_json::json!("bad")));
        assert!(server.handle_notification(bad_open).is_none());

        // 2. didChange with invalid params
        let bad_change =
            Notification::new("textDocument/didChange", Some(serde_json::json!("bad")));
        assert!(server.handle_notification(bad_change).is_none());

        // 3. didChange on document not in cache
        let missing_doc_change = Notification::new(
            "textDocument/didChange",
            Some(serde_json::json!({
                "textDocument": { "uri": "file:///none.hcl", "version": 2 },
                "contentChanges": [{ "text": "new text" }]
            })),
        );
        assert!(server.handle_notification(missing_doc_change).is_none());

        // 4. didOpen valid with diagnostic error to exercise diagnostic json mapping
        let err_open = Notification::new(
            "textDocument/didOpen",
            Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///err.hcl",
                    "languageId": "hcl",
                    "version": 1,
                    "text": "bad_syntax = \n",
                }
            })),
        );
        assert!(server.handle_notification(err_open).is_some());

        // 5. didOpen valid clean
        let good_open = Notification::new(
            "textDocument/didOpen",
            Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///test.hcl",
                    "languageId": "hcl",
                    "version": 1,
                    "text": "x = 1\n",
                }
            })),
        );
        assert!(server.handle_notification(good_open).is_some());

        // 6. didChange valid
        let good_change = Notification::new(
            "textDocument/didChange",
            Some(serde_json::json!({
                "textDocument": { "uri": "file:///test.hcl", "version": 2 },
                "contentChanges": [{ "text": "x = 2\n" }]
            })),
        );
        assert!(server.handle_notification(good_change).is_some());

        // 7. didClose with invalid params
        let bad_close = Notification::new("textDocument/didClose", Some(serde_json::json!("bad")));
        assert!(server.handle_notification(bad_close).is_none());

        // 8. didClose valid
        let good_close = Notification::new(
            "textDocument/didClose",
            Some(serde_json::json!({
                "textDocument": { "uri": "file:///test.hcl" }
            })),
        );
        assert!(server.handle_notification(good_close).is_none());
        assert!(server.cache.get_document("file:///test.hcl").is_none());

        // 9. Unknown notification
        let unknown_notif = Notification::new("unknown/notif", None);
        assert!(server.handle_notification(unknown_notif).is_none());
    }

    #[test]
    fn test_server_run_stream() {
        let mut server = LspServer::new();

        let mut input_buf = Vec::new();
        let _ = Transport::write_message(
            &mut input_buf,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        let _ = Transport::write_message(
            &mut input_buf,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///stream.hcl","languageId":"hcl","version":1,"text":"val = 10\n"}}}"#,
        );
        let _ = Transport::write_message(
            &mut input_buf,
            r#"{"jsonrpc":"2.0","method":"initialized","params":null}"#,
        );
        let _ = Transport::write_message(&mut input_buf, "{\"plain\": 123}");
        let _ = Transport::write_message(
            &mut input_buf,
            r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
        );

        let mut output_buf = Vec::new();
        let res = server.run_stream(Cursor::new(input_buf), &mut output_buf);
        assert!(res.is_ok());
        assert_ne!(output_buf.len(), 0);

        // Stream that ends with clean EOF (no exit notification)
        let mut eof_buf = Vec::new();
        let _ = Transport::write_message(
            &mut eof_buf,
            r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
        );
        let mut eof_out = Vec::new();
        assert!(
            server
                .run_stream(Cursor::new(eof_buf), &mut eof_out)
                .is_ok()
        );
    }

    #[test]
    fn test_server_run_tcp_lifecycle() {
        let mut server = LspServer::new();

        for target in ["invalid-address:99999", "127.0.0.1:0"] {
            match TcpListener::bind(target) {
                Ok(listener) => {
                    let port = listener.local_addr().map_or(0, |a| a.port());
                    drop(listener);

                    let addr = format!("127.0.0.1:{port}");
                    let thread_addr = addr.clone();
                    let client_handle = std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        for is_err in [true, false] {
                            let target_addr = if is_err { "127.0.0.1:1" } else { &thread_addr };
                            if let Ok(mut stream) = std::net::TcpStream::connect(target_addr) {
                                let exit_str = r#"{"jsonrpc":"2.0","method":"exit","params":null}"#;
                                let _ = Transport::write_message(&mut stream, exit_str);
                            }
                        }
                    });

                    assert!(server.run_tcp(&addr).is_ok());
                    let _ = client_handle.join();
                }
                Err(_) => {
                    assert!(server.run_tcp(target).is_err());
                }
            }
        }
    }
}
