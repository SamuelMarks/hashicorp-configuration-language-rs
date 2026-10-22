//! Error types for the HCL-RS crate.
//!
//! Provides [`HclError`], the single strongly-typed error enum used across all crates,
//! binaries, evaluators, serializers, and language server components in this workspace.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use derive_more::derive::{Display, Error, From};

/// A strongly-typed error enum representing all possible failure states across the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Display, Error, From)]
pub enum HclError {
    /// An error that occurs during lexical analysis.
    #[display("Lexical error: {_0}")]
    #[from(ignore)]
    Lex(#[error(ignore)] String),
    /// An error that occurs during parsing.
    #[display("Parse error: {_0}")]
    #[from(ignore)]
    Parse(#[error(ignore)] String),
    /// An error that occurs during evaluation.
    #[display("Evaluation error: {_0}")]
    #[from(ignore)]
    Eval(#[error(ignore)] String),
    /// An error that occurs during decoding.
    #[display("Decode error: {_0}")]
    #[from(ignore)]
    Decode(#[error(ignore)] String),
    /// An error that occurs during formatting or serialization.
    #[display("Format error: {_0}")]
    #[from(ignore)]
    Format(#[error(ignore)] String),
    /// An error that occurs due to unexpected or invalid types.
    #[display("Type error: {_0}")]
    #[from(ignore)]
    Type(#[error(ignore)] String),
    /// An error that occurs due to missing fields.
    #[display("Missing field: {_0}")]
    #[from(ignore)]
    MissingField(#[error(ignore)] String),
    /// An error that occurs when a function argument does not match the expected type.
    #[display("Function argument mismatch: {_0}")]
    #[from(ignore)]
    FunctionArgMismatch(#[error(ignore)] String),
    /// An error that occurs during dynamic block processing.
    #[display("Dynamic block error: {_0}")]
    #[from(ignore)]
    DynamicBlock(#[error(ignore)] String),
    /// An error that occurs during schema validation.
    #[display("Schema error: {_0}")]
    #[from(ignore)]
    Schema(#[error(ignore)] String),
    /// An error that occurs during traversal analysis or conversion.
    #[display("Traversal error: {_0}")]
    #[from(ignore)]
    Traversal(#[error(ignore)] String),
    /// An error that occurs when a dependency cycle is detected.
    #[display("Cyclic dependency detected: {_0}")]
    #[from(ignore)]
    CyclicDependency(#[error(ignore)] String),
    /// An error that occurs during condition assertion or declarative validation.
    #[display("Validation error: {_0}")]
    #[from(ignore)]
    Validation(#[error(ignore)] String),
    /// An error that occurs when failing to downcast an encapsulated value.
    #[display("Capsule downcast error: expected {expected}, got {actual}")]
    #[from(ignore)]
    CapsuleDowncast {
        /// The expected type description.
        expected: &'static str,
        /// The actual type description.
        actual: String,
    },
    /// A general error that occurs during capsule operations.
    #[display("Capsule error: {_0}")]
    #[from(ignore)]
    Capsule(#[error(ignore)] String),
    /// An I/O error that occurs when reading or writing files or streams.
    #[display("I/O error: {_0}")]
    #[from(ignore)]
    Io(#[error(ignore)] String),
    /// A virtual filesystem error.
    #[display("Filesystem error: {_0}")]
    #[from(ignore)]
    FileSystem(#[error(ignore)] String),
    /// An error that occurs during template parsing or directive evaluation.
    #[display("Template error: {_0}")]
    #[from(ignore)]
    Template(#[error(ignore)] String),
    /// An error that occurs during heredoc processing or indentation normalization.
    #[display("Heredoc error: {_0}")]
    #[from(ignore)]
    Heredoc(#[error(ignore)] String),
    /// An error that occurs during `MessagePack` encoding.
    #[display("MessagePack encode error: {_0}")]
    #[from(ignore)]
    MsgPackEncode(#[error(ignore)] String),
    /// An error that occurs during `MessagePack` decoding.
    #[display("MessagePack decode error: {_0}")]
    #[from(ignore)]
    MsgPackDecode(#[error(ignore)] String),
    /// An error that occurs during namespace resolution or invocation.
    #[display("Namespace error: {_0}")]
    #[from(ignore)]
    NamespaceError(#[error(ignore)] String),
    /// An error that occurs during partial expression evaluation.
    #[display("Partial evaluation error: {_0}")]
    #[from(ignore)]
    PartialEval(#[error(ignore)] String),
    /// An error that occurs during payload encoding or decoding.
    #[display("Encoding error: {_0}")]
    #[from(ignore)]
    Encoding(#[error(ignore)] String),
    /// An error that occurs during nested path traversal.
    #[display("Path error at {path}: {message}")]
    #[from(ignore)]
    PathError {
        /// The path where the error occurred.
        path: String,
        /// The error message.
        message: String,
    },
    /// An error that occurs during cty JSON serialization or deserialization.
    #[display("cty JSON error: {_0}")]
    #[from(ignore)]
    CtyJson(#[error(ignore)] String),
    /// An error that occurs during Unicode processing or grapheme cluster segmentation.
    #[display("Unicode error: {_0}")]
    #[from(ignore)]
    Unicode(#[error(ignore)] String),
    /// An error that occurs when an unsupported character encoding is requested.
    #[display("Unsupported encoding: {_0}")]
    #[from(ignore)]
    UnsupportedEncoding(#[error(ignore)] String),
    /// An error that occurs when character encoding conversion fails.
    #[display("Encoding conversion error from {from} to {to}: {reason}")]
    #[from(ignore)]
    EncodingConversion {
        /// The source encoding.
        from: String,
        /// The target encoding.
        to: String,
        /// The reason for failure.
        reason: String,
    },
    /// An error that occurs when a referenced CST node is not found.
    #[display("CST node not found: {_0}")]
    #[from(ignore)]
    CstNodeNotFound(#[error(ignore)] String),
    /// An error that occurs when attempting an invalid CST positioning or insertion.
    #[display("CST invalid position: {_0}")]
    #[from(ignore)]
    CstInvalidPosition(#[error(ignore)] String),
    /// An error that occurs when a requested capsule method does not exist.
    #[display("Capsule method '{method}' not found on capsule type '{capsule_type}'")]
    #[from(ignore)]
    CapsuleMethodNotFound {
        /// The capsule type name.
        capsule_type: &'static str,
        /// The method name.
        method: String,
    },
    /// An error that occurs when a capsule method execution fails.
    #[display("Capsule method '{method}' on capsule type '{capsule_type}' failed: {reason}")]
    #[from(ignore)]
    CapsuleMethodError {
        /// The capsule type name.
        capsule_type: &'static str,
        /// The method name.
        method: String,
        /// The error reason.
        reason: String,
    },
    /// A linting diagnostic or rule violation.
    #[display("Lint warning: {_0}")]
    #[from(ignore)]
    Lint(#[error(ignore)] String),
    /// An error that occurs during command-line argument parsing or CLI execution.
    #[display("CLI error: {_0}")]
    #[from(ignore)]
    CliArgs(#[error(ignore)] String),
    /// An LSP framing or JSON-RPC protocol validation error.
    #[display("Protocol error: {_0}")]
    #[from(ignore)]
    LspProtocol(#[error(ignore)] String),
    /// The requested document URI was not found in the LSP virtual cache.
    #[display("Document not found: {_0}")]
    #[from(ignore)]
    LspDocumentNotFound(#[error(ignore)] String),
    /// A general LSP server state or lifecycle error.
    #[display("Server error: {_0}")]
    #[from(ignore)]
    LspServer(#[error(ignore)] String),
    /// A JSON serialization or deserialization error.
    #[display("JSON error: {_0}")]
    #[from(ignore)]
    Json(#[error(ignore)] String),
    /// A Serde serialization error.
    #[display("Serialize error: {_0}")]
    #[from(ignore)]
    SerdeSerialize(#[error(ignore)] String),
    /// A Serde deserialization error.
    #[display("Deserialize error: {_0}")]
    #[from(ignore)]
    SerdeDeserialize(#[error(ignore)] String),
    /// A null pointer was passed across the C FFI boundary.
    #[display("FFI null pointer error in {_0}")]
    #[from(ignore)]
    FfiNullPointer(#[error(ignore)] &'static str),
    /// An invalid UTF-8 string was encountered across the C FFI boundary.
    #[display("FFI UTF-8 error: {_0}")]
    #[from(ignore)]
    FfiUtf8(#[error(ignore)] String),
    /// A WebAssembly boundary conversion or execution error.
    #[display("Wasm interop error: {_0}")]
    #[from(ignore)]
    WasmInterop(#[error(ignore)] String),
    /// A collection of structured HCL diagnostics wrapped as an error.
    #[display("{_0}")]
    Diagnostics(#[error(not(source))] Box<crate::diagnostic::Diagnostics>),
}

impl From<crate::diagnostic::Diagnostics> for HclError {
    fn from(diags: crate::diagnostic::Diagnostics) -> Self {
        Self::Diagnostics(Box::new(diags))
    }
}

impl From<HclError> for crate::diagnostic::Diagnostics {
    fn from(err: HclError) -> Self {
        match err {
            HclError::Diagnostics(diags) => *diags,
            other => Self::from(crate::diagnostic::Diagnostic::error(
                "Error",
                other.to_string(),
                crate::span::Span::default(),
            )),
        }
    }
}

impl From<std::io::Error> for HclError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<std::fmt::Error> for HclError {
    fn from(err: std::fmt::Error) -> Self {
        Self::Format(err.to_string())
    }
}

impl From<serde_json::Error> for HclError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err.to_string())
    }
}

impl From<serde_yaml::Error> for HclError {
    fn from(err: serde_yaml::Error) -> Self {
        Self::Encoding(err.to_string())
    }
}

impl From<csv::Error> for HclError {
    fn from(err: csv::Error) -> Self {
        Self::Encoding(err.to_string())
    }
}

impl From<regex::Error> for HclError {
    fn from(err: regex::Error) -> Self {
        Self::Eval(err.to_string())
    }
}

impl From<uuid::Error> for HclError {
    fn from(err: uuid::Error) -> Self {
        Self::Eval(err.to_string())
    }
}

impl From<ipnet::AddrParseError> for HclError {
    fn from(err: ipnet::AddrParseError) -> Self {
        Self::Eval(err.to_string())
    }
}

impl From<base64::DecodeError> for HclError {
    fn from(err: base64::DecodeError) -> Self {
        Self::Encoding(err.to_string())
    }
}

impl From<bigdecimal::ParseBigDecimalError> for HclError {
    fn from(err: bigdecimal::ParseBigDecimalError) -> Self {
        Self::Parse(err.to_string())
    }
}

impl From<std::str::Utf8Error> for HclError {
    fn from(err: std::str::Utf8Error) -> Self {
        Self::Unicode(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for HclError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self::Unicode(err.to_string())
    }
}

impl serde::ser::Error for HclError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self::SerdeSerialize(msg.to_string())
    }
}

impl serde::de::Error for HclError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self::SerdeDeserialize(msg.to_string())
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, Diagnostics};
    use crate::span::Span;
    use serde::de::Error as DeError;
    use serde::ser::Error as SerError;
    use std::str::FromStr;

    #[test]
    fn test_hcl_error_display_core() {
        let err_cm_nf = HclError::CapsuleMethodNotFound {
            capsule_type: "matrix",
            method: "invert".to_string(),
        };
        assert!(
            err_cm_nf
                .to_string()
                .contains("Capsule method 'invert' not found on capsule type 'matrix'")
        );
        let err_cm_err = HclError::CapsuleMethodError {
            capsule_type: "matrix",
            method: "invert".to_string(),
            reason: "singular matrix".to_string(),
        };
        assert!(
            err_cm_err.to_string().contains(
                "Capsule method 'invert' on capsule type 'matrix' failed: singular matrix"
            )
        );
        let err_cst_nf = HclError::CstNodeNotFound("target_attr".to_string());
        assert!(
            err_cst_nf
                .to_string()
                .contains("CST node not found: target_attr")
        );
        let err_cst_pos = HclError::CstInvalidPosition("out of bounds".to_string());
        assert!(
            err_cst_pos
                .to_string()
                .contains("CST invalid position: out of bounds")
        );
        let err_enc = HclError::UnsupportedEncoding("ebcdic".to_string());
        assert!(err_enc.to_string().contains("Unsupported encoding: ebcdic"));
        let err_conv = HclError::EncodingConversion {
            from: "UTF-8".to_string(),
            to: "ASCII".to_string(),
            reason: "unrepresentable character".to_string(),
        };
        assert!(
            err_conv.to_string().contains(
                "Encoding conversion error from UTF-8 to ASCII: unrepresentable character"
            )
        );
        let err_u = HclError::Unicode("invalid grapheme".to_string());
        assert!(
            err_u
                .to_string()
                .contains("Unicode error: invalid grapheme")
        );
        let err = HclError::Lex("invalid character".to_string());
        assert!(err.to_string().contains("Lexical error: invalid character"));
        let err_hd = HclError::Heredoc("mixed indentation".to_string());
        assert!(
            err_hd
                .to_string()
                .contains("Heredoc error: mixed indentation")
        );
        let err = HclError::DynamicBlock("missing for_each".to_string());
        assert!(
            err.to_string()
                .contains("Dynamic block error: missing for_each")
        );
        let err = HclError::Schema("unsupported argument".to_string());
        assert!(
            err.to_string()
                .contains("Schema error: unsupported argument")
        );
        let err = HclError::Traversal("invalid traversal root".to_string());
        assert!(
            err.to_string()
                .contains("Traversal error: invalid traversal root")
        );
        let err = HclError::CyclicDependency("a -> b -> a".to_string());
        assert!(
            err.to_string()
                .contains("Cyclic dependency detected: a -> b -> a")
        );
        let err = HclError::Validation("check failed".to_string());
        assert!(err.to_string().contains("Validation error: check failed"));
        let err = HclError::CapsuleDowncast {
            expected: "MyType",
            actual: "capsule(OtherType)".to_string(),
        };
        assert!(
            err.to_string()
                .contains("Capsule downcast error: expected MyType, got capsule(OtherType)")
        );
        let err = HclError::Capsule("operation failed".to_string());
        assert!(err.to_string().contains("Capsule error: operation failed"));
        let err = HclError::Io("file not found".to_string());
        assert!(err.to_string().contains("I/O error: file not found"));
        let err_fs = HclError::FileSystem("permission denied".to_string());
        assert!(
            err_fs
                .to_string()
                .contains("Filesystem error: permission denied")
        );
    }

    #[test]
    fn test_hcl_error_display_extended() {
        let err = HclError::Parse("unexpected token".to_string());
        assert!(err.to_string().contains("Parse error: unexpected token"));
        let err = HclError::Eval("undefined variable".to_string());
        assert!(
            err.to_string()
                .contains("Evaluation error: undefined variable")
        );
        let err = HclError::Decode("invalid structure".to_string());
        assert!(err.to_string().contains("Decode error: invalid structure"));
        let err = HclError::Format("cannot serialize".to_string());
        assert!(err.to_string().contains("Format error: cannot serialize"));
        let err = HclError::Type("expected string".to_string());
        assert!(err.to_string().contains("Type error: expected string"));
        let err = HclError::MissingField("name".to_string());
        assert!(err.to_string().contains("Missing field: name"));
        let err = HclError::FunctionArgMismatch("expected number".to_string());
        assert!(
            err.to_string()
                .contains("Function argument mismatch: expected number")
        );
        let err = HclError::Template("unclosed directive".to_string());
        assert!(
            err.to_string()
                .contains("Template error: unclosed directive")
        );
        let err_mp_enc = HclError::MsgPackEncode("buffer overflow".to_string());
        assert!(
            err_mp_enc
                .to_string()
                .contains("MessagePack encode error: buffer overflow")
        );
        let err_mp_dec = HclError::MsgPackDecode("invalid byte".to_string());
        assert!(
            err_mp_dec
                .to_string()
                .contains("MessagePack decode error: invalid byte")
        );
        let err_path = HclError::PathError {
            path: "server[0].host".to_string(),
            message: "not found".to_string(),
        };
        assert!(
            err_path
                .to_string()
                .contains("Path error at server[0].host: not found")
        );
        let err_cty_json = HclError::CtyJson("invalid type signature".to_string());
        assert!(
            err_cty_json
                .to_string()
                .contains("cty JSON error: invalid type signature")
        );
        let err_namespace = HclError::NamespaceError("unknown namespace provider".to_string());
        assert!(
            err_namespace
                .to_string()
                .contains("Namespace error: unknown namespace provider")
        );
        let err_partial = HclError::PartialEval("unresolvable variable".to_string());
        assert!(
            err_partial
                .to_string()
                .contains("Partial evaluation error: unresolvable variable")
        );
        let err_enc = HclError::Encoding("corrupted gzip stream".to_string());
        assert!(
            err_enc
                .to_string()
                .contains("Encoding error: corrupted gzip stream")
        );
        let err_lint = HclError::Lint("unused variable".to_string());
        assert!(
            err_lint
                .to_string()
                .contains("Lint warning: unused variable")
        );
    }

    #[test]
    fn test_hcl_error_display_cli_lsp_ffi_wasm() {
        let err_cli = HclError::CliArgs("invalid flag".to_string());
        assert!(err_cli.to_string().contains("CLI error: invalid flag"));
        let err_lsp_proto = HclError::LspProtocol("missing content-length".to_string());
        assert!(
            err_lsp_proto
                .to_string()
                .contains("Protocol error: missing content-length")
        );
        let err_lsp_doc = HclError::LspDocumentNotFound("file:///a.hcl".to_string());
        assert!(
            err_lsp_doc
                .to_string()
                .contains("Document not found: file:///a.hcl")
        );
        let err_lsp_srv = HclError::LspServer("not initialized".to_string());
        assert!(
            err_lsp_srv
                .to_string()
                .contains("Server error: not initialized")
        );
        let err_json = HclError::Json("bad json".to_string());
        assert!(err_json.to_string().contains("JSON error: bad json"));
        let err_ser = <HclError as SerError>::custom("ser fail");
        assert_eq!(err_ser, HclError::SerdeSerialize("ser fail".to_string()));
        assert!(err_ser.to_string().contains("Serialize error: ser fail"));
        let err_deser = <HclError as DeError>::custom("de fail");
        assert_eq!(err_deser, HclError::SerdeDeserialize("de fail".to_string()));
        assert!(err_deser.to_string().contains("Deserialize error: de fail"));
        let err_ffi_null = HclError::FfiNullPointer("hcl_parse");
        assert!(
            err_ffi_null
                .to_string()
                .contains("FFI null pointer error in hcl_parse")
        );
        let err_ffi_utf8 = HclError::FfiUtf8("invalid utf8".to_string());
        assert!(
            err_ffi_utf8
                .to_string()
                .contains("FFI UTF-8 error: invalid utf8")
        );
        let err_wasm = HclError::WasmInterop("js err".to_string());
        assert!(err_wasm.to_string().contains("Wasm interop error: js err"));
    }

    #[test]
    fn test_hcl_error_traits_and_conversions() -> Result<(), HclError> {
        let err = HclError::Lex("test".to_string());
        let err_clone = Clone::clone(&err);
        assert_eq!(err, err_clone);
        let err_hd = HclError::Heredoc("test".to_string());
        let err_hd_clone = Clone::clone(&err_hd);
        assert_eq!(err_hd, err_hd_clone);
        let err_mp = HclError::MsgPackEncode("test".to_string());
        assert_eq!(err_mp, Clone::clone(&err_mp));
        let std_err: &dyn std::error::Error = &err;
        assert!(std_err.to_string().contains("Lexical error: test"));

        let mut diags = Diagnostics::new();
        diags.push(Diagnostic::error("diag err", "detail", Span::default()));
        let from_diags = HclError::from(diags.clone());
        assert!(from_diags.to_string().contains("diag err"));
        let from_box_diags = HclError::from(Box::new(diags.clone()));
        assert_eq!(from_diags, from_box_diags);
        let roundtrip_diags = Diagnostics::from(from_diags);
        assert_eq!(roundtrip_diags, diags);
        let single_diag = Diagnostics::from(HclError::Eval("eval fail".to_string()));
        assert!(single_diag.has_errors());

        let io_err = std::io::Error::other("disk full");
        assert!(HclError::from(io_err).to_string().contains("disk full"));

        let fmt_err = std::fmt::Error;
        assert!(HclError::from(fmt_err).to_string().contains("Format error"));

        let json_err = serde_json::from_str::<serde_json::Value>("[invalid")
            .err()
            .ok_or_else(|| HclError::Eval("expected json error".to_string()))?;
        assert!(HclError::from(json_err).to_string().contains("JSON error"));

        let yaml_err = serde_yaml::from_str::<serde_yaml::Value>(":\n  - [\n")
            .err()
            .ok_or_else(|| HclError::Eval("expected yaml error".to_string()))?;
        assert!(
            HclError::from(yaml_err)
                .to_string()
                .contains("Encoding error")
        );

        let mut csv_rdr = csv::ReaderBuilder::new().from_reader(&b"a,b\n\"unclosed"[..]);
        let csv_err = csv_rdr
            .records()
            .next()
            .and_then(Result::err)
            .ok_or_else(|| HclError::Eval("expected csv error".to_string()))?;
        assert!(
            HclError::from(csv_err)
                .to_string()
                .contains("Encoding error")
        );
        Ok(())
    }

    #[test]
    fn test_hcl_error_conversions_stdlib() -> Result<(), HclError> {
        let bad_regex_pattern = String::from("[unclosed");
        let re_err = regex::Regex::new(&bad_regex_pattern)
            .err()
            .ok_or_else(|| HclError::Eval("expected regex error".to_string()))?;
        assert!(
            HclError::from(re_err)
                .to_string()
                .contains("Evaluation error")
        );

        let uuid_err = uuid::Uuid::parse_str("not-a-uuid")
            .err()
            .ok_or_else(|| HclError::Eval("expected uuid error".to_string()))?;
        assert!(
            HclError::from(uuid_err)
                .to_string()
                .contains("Evaluation error")
        );

        let ip_err = ipnet::IpNet::from_str("not-an-ip")
            .err()
            .ok_or_else(|| HclError::Eval("expected ip error".to_string()))?;
        assert!(
            HclError::from(ip_err)
                .to_string()
                .contains("Evaluation error")
        );

        let base64_err = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, "!!!")
            .err()
            .ok_or_else(|| HclError::Eval("expected base64 error".to_string()))?;
        assert!(
            HclError::from(base64_err)
                .to_string()
                .contains("Encoding error")
        );

        let decimal_err = bigdecimal::BigDecimal::from_str("not-a-number")
            .err()
            .ok_or_else(|| HclError::Eval("expected bigdecimal error".to_string()))?;
        assert!(
            HclError::from(decimal_err)
                .to_string()
                .contains("Parse error")
        );

        let bad_bytes = vec![0xFF_u8, 0xFF_u8];
        let utf8_err = std::str::from_utf8(&bad_bytes)
            .err()
            .ok_or_else(|| HclError::Eval("expected utf8 error".to_string()))?;
        assert!(
            HclError::from(utf8_err)
                .to_string()
                .contains("Unicode error")
        );

        let string_utf8_err = String::from_utf8(vec![0xFF, 0xFF])
            .err()
            .ok_or_else(|| HclError::Eval("expected from_utf8 error".to_string()))?;
        assert!(
            HclError::from(string_utf8_err)
                .to_string()
                .contains("Unicode error")
        );
        Ok(())
    }
}
