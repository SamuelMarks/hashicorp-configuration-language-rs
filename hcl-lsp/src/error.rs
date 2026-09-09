//! Error types for `hcl-lsp`.

use derive_more::derive::{Display, Error};

/// Strongly-typed error enum representing all possible failure conditions in `hcl-lsp`.
#[derive(Debug, Clone, PartialEq, Eq, Display, Error)]
pub enum LspError {
    /// An I/O error occurred during transport communication.
    #[display("I/O error: {_0}")]
    Io(#[error(ignore)] String),

    /// A JSON serialization or deserialization error occurred.
    #[display("JSON error: {_0}")]
    Json(#[error(ignore)] String),

    /// An LSP framing or protocol validation error occurred.
    #[display("Protocol error: {_0}")]
    Protocol(#[error(ignore)] String),

    /// The requested document URI was not found in the virtual cache.
    #[display("Document not found: {_0}")]
    DocumentNotFound(#[error(ignore)] String),

    /// A general server failure or state mismatch occurred.
    #[display("Server error: {_0}")]
    Server(#[error(ignore)] String),

    /// An HCL parsing, analysis, or evaluation error occurred.
    #[display("HCL error: {_0}")]
    Hcl(#[error(ignore)] String),
}

impl From<std::io::Error> for LspError {
    fn from(err: std::io::Error) -> Self {
        LspError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for LspError {
    fn from(err: serde_json::Error) -> Self {
        LspError::Json(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_error_display() {
        let err_io = LspError::Io("connection reset".to_string());
        assert!(err_io.to_string().contains("connection reset"));

        let err_json = LspError::Json("bad syntax".to_string());
        assert!(err_json.to_string().contains("bad syntax"));

        let err_proto = LspError::Protocol("missing content-length".to_string());
        assert!(err_proto.to_string().contains("missing content-length"));

        let err_doc = LspError::DocumentNotFound("file:///a.hcl".to_string());
        assert!(err_doc.to_string().contains("file:///a.hcl"));

        let err_srv = LspError::Server("not initialized".to_string());
        assert!(err_srv.to_string().contains("not initialized"));

        let err_hcl = LspError::Hcl("parse failed".to_string());
        assert!(err_hcl.to_string().contains("parse failed"));

        // Test From implementations
        let io_err = std::io::Error::other("simulated io error");
        let from_io = LspError::from(io_err);
        assert!(from_io.to_string().contains("simulated io error"));

        let serde_err = serde_json::from_str::<serde_json::Value>("{ invalid")
            .expect_err("expected json error");
        let from_json = LspError::from(serde_err);
        assert!(from_json.to_string().contains("JSON error"));
    }
}
