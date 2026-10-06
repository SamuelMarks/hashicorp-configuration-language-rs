//! Error types for the HCL-RS crate.
use derive_more::derive::{Display, Error};
/// A strongly-typed error enum representing all possible failure states.
#[derive(Debug, Clone, PartialEq, Eq, Display, Error)]
pub enum HclError {
    /// An error that occurs during lexical analysis.
    #[display("Lexical error: {_0}")]
    Lex(#[error(ignore)] String),
    /// An error that occurs during parsing.
    #[display("Parse error: {_0}")]
    Parse(#[error(ignore)] String),
    /// An error that occurs during evaluation.
    #[display("Evaluation error: {_0}")]
    Eval(#[error(ignore)] String),
    /// An error that occurs during decoding.
    #[display("Decode error: {_0}")]
    Decode(#[error(ignore)] String),
    /// An error that occurs during formatting or serialization.
    #[display("Format error: {_0}")]
    Format(#[error(ignore)] String),
    /// An error that occurs due to unexpected or invalid types.
    #[display("Type error: {_0}")]
    Type(#[error(ignore)] String),
    /// An error that occurs due to missing fields.
    #[display("Missing field: {_0}")]
    MissingField(#[error(ignore)] String),
    /// An error that occurs when a function argument does not match the expected type.
    #[display("Function argument mismatch: {_0}")]
    FunctionArgMismatch(#[error(ignore)] String),
    /// An error that occurs during dynamic block processing.
    #[display("Dynamic block error: {_0}")]
    DynamicBlock(#[error(ignore)] String),
    /// An error that occurs during schema validation.
    #[display("Schema error: {_0}")]
    Schema(#[error(ignore)] String),
    /// An error that occurs during traversal analysis or conversion.
    #[display("Traversal error: {_0}")]
    Traversal(#[error(ignore)] String),
    /// An error that occurs when a dependency cycle is detected.
    #[display("Cyclic dependency detected: {_0}")]
    CyclicDependency(#[error(ignore)] String),
    /// An error that occurs during condition assertion or declarative validation.
    #[display("Validation error: {_0}")]
    Validation(#[error(ignore)] String),
    /// An error that occurs when failing to downcast an encapsulated value.
    #[display("Capsule downcast error: expected {expected}, got {actual}")]
    CapsuleDowncast {
        /// The expected type description.
        expected: &'static str,
        /// The actual type description.
        actual: String,
    },
    /// A general error that occurs during capsule operations.
    #[display("Capsule error: {_0}")]
    Capsule(#[error(ignore)] String),
    /// An I/O error that occurs when reading or writing files.
    #[display("I/O error: {_0}")]
    Io(#[error(ignore)] String),
    /// A virtual filesystem error.
    #[display("Filesystem error: {_0}")]
    FileSystem(#[error(ignore)] String),
    /// An error that occurs during template parsing or directive evaluation.
    #[display("Template error: {_0}")]
    Template(#[error(ignore)] String),
    /// An error that occurs during heredoc processing or indentation normalization.
    #[display("Heredoc error: {_0}")]
    Heredoc(#[error(ignore)] String),
    /// An error that occurs during `MessagePack` encoding.
    #[display("MessagePack encode error: {_0}")]
    MsgPackEncode(#[error(ignore)] String),
    /// An error that occurs during `MessagePack` decoding.
    #[display("MessagePack decode error: {_0}")]
    MsgPackDecode(#[error(ignore)] String),
    /// An error that occurs during namespace resolution or invocation.
    #[display("Namespace error: {_0}")]
    NamespaceError(#[error(ignore)] String),
    /// An error that occurs during partial expression evaluation.
    #[display("Partial evaluation error: {_0}")]
    PartialEval(#[error(ignore)] String),
    /// An error that occurs during payload encoding or decoding.
    #[display("Encoding error: {_0}")]
    Encoding(#[error(ignore)] String),
    /// An error that occurs during nested path traversal.
    #[display("Path error at {path}: {message}")]
    PathError {
        /// The path where the error occurred.
        path: String,
        /// The error message.
        message: String,
    },
    /// An error that occurs during cty JSON serialization or deserialization.
    #[display("cty JSON error: {_0}")]
    CtyJson(#[error(ignore)] String),
    /// An error that occurs during Unicode processing or grapheme cluster segmentation.
    #[display("Unicode error: {_0}")]
    Unicode(#[error(ignore)] String),
    /// An error that occurs when an unsupported character encoding is requested.
    #[display("Unsupported encoding: {_0}")]
    UnsupportedEncoding(#[error(ignore)] String),
    /// An error that occurs when character encoding conversion fails.
    #[display("Encoding conversion error from {from} to {to}: {reason}")]
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
    CstNodeNotFound(#[error(ignore)] String),
    /// An error that occurs when attempting an invalid CST positioning or insertion.
    #[display("CST invalid position: {_0}")]
    CstInvalidPosition(#[error(ignore)] String),
    /// An error that occurs when a requested capsule method does not exist.
    #[display("Capsule method '{method}' not found on capsule type '{capsule_type}'")]
    CapsuleMethodNotFound {
        /// The capsule type name.
        capsule_type: &'static str,
        /// The method name.
        method: String,
    },
    /// An error that occurs when a capsule method execution fails.
    #[display("Capsule method '{method}' on capsule type '{capsule_type}' failed: {reason}")]
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
    Lint(#[error(ignore)] String),
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
    #[test]
    fn test_hcl_error_display() {
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
        let err_ns = HclError::NamespaceError("unknown namespace provider".to_string());
        assert!(
            err_ns
                .to_string()
                .contains("Namespace error: unknown namespace provider")
        );
        let err_pe = HclError::PartialEval("unresolvable variable".to_string());
        assert!(
            err_pe
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
    fn test_hcl_error_traits() {
        let err = HclError::Lex("test".to_string());
        let err_clone = err.clone();
        assert_eq!(err, err_clone);
        let err_hd = HclError::Heredoc("test".to_string());
        let err_hd_clone = err_hd.clone();
        assert_eq!(err_hd, err_hd_clone);
        let err_mp = HclError::MsgPackEncode("test".to_string());
        assert_eq!(err_mp, err_mp.clone());
        let std_err: &dyn std::error::Error = &err;
        assert!(std_err.to_string().contains("Lexical error: test"));
    }
}
