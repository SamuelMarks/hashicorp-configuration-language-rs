//! Error types for `hcl-lsp`.
//!
//! Re-exports [`HclError`] from the core crate as the unified error type across the workspace,
//! along with [`LspError`] as a type alias for backward compatibility.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub use hashicorp_configuration_language_rs::error::HclError;

/// Type alias to the unified workspace [`HclError`] enum.
pub type LspError = HclError;

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_error_display_and_conversions() {
        let err_io = LspError::Io("broken pipe".to_string());
        assert!(err_io.to_string().contains("broken pipe"));
        let err_json = LspError::Json("bad syntax".to_string());
        assert!(err_json.to_string().contains("bad syntax"));
        let err_proto = LspError::LspProtocol("missing content-length".to_string());
        assert!(err_proto.to_string().contains("missing content-length"));
        let err_doc = LspError::LspDocumentNotFound("file:///main.tf".to_string());
        assert!(err_doc.to_string().contains("file:///main.tf"));
        let err_srv = LspError::LspServer("not initialized".to_string());
        assert!(err_srv.to_string().contains("not initialized"));
        assert_eq!(err_srv, err_srv.clone());

        let io_err = std::io::Error::other("io err");
        let from_io = LspError::from(io_err);
        assert!(from_io.to_string().contains("io err"));

        if let Err(serde_err) = serde_json::from_str::<serde_json::Value>("{ invalid") {
            let from_json = LspError::from(serde_err);
            assert!(from_json.to_string().contains("JSON error"));
        }
    }
}
