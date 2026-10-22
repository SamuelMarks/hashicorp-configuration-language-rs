//! JSON-RPC 2.0 transport layer with LSP header framing.
//!
//! Handles `Content-Length` message framing over any standard
//! [`Read`](std::io::Read) and [`Write`] streams, supporting both stdio and TCP.
#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::error::HclError;
use std::io::{BufRead, Write};

/// JSON-RPC transport helper for reading and writing framed LSP messages.
pub struct Transport;

impl Transport {
    /// Reads a single framed JSON-RPC message from a buffered reader.
    ///
    /// # Arguments
    /// * `reader` - The buffered reader to read from.
    ///
    /// # Returns
    /// * `Ok(Some(content))` - A successfully read JSON payload string.
    /// * `Ok(None)` - End-of-file reached cleanly before any headers.
    ///
    /// # Errors
    /// Returns [`HclError::LspProtocol`] or [`HclError::Io`] if headers are malformed or reading fails.
    pub fn read_message(reader: &mut dyn BufRead) -> Result<Option<String>, HclError> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let bytes_read = reader
                .read_line(&mut line)
                .map_err(|e| HclError::Io(e.to_string()))?;
            if bytes_read == 0 {
                if content_length.is_none() {
                    return Ok(None);
                }
                return Err(HclError::LspProtocol(
                    "Unexpected EOF while reading headers".to_string(),
                ));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(stripped) = trimmed.strip_prefix("Content-Length:") {
                let len_str = stripped.trim();
                let len = len_str.parse::<usize>().map_err(|_| {
                    HclError::LspProtocol(format!("Invalid Content-Length value: {len_str}"))
                })?;
                content_length = Some(len);
            }
        }
        let Some(len) = content_length else {
            return Err(HclError::LspProtocol(
                "Missing Content-Length header".to_string(),
            ));
        };
        let mut buffer = vec![0u8; len];
        reader
            .read_exact(&mut buffer)
            .map_err(|e| HclError::Io(format!("Failed to read message body: {e}")))?;
        let content = String::from_utf8(buffer)
            .map_err(|e| HclError::LspProtocol(format!("Invalid UTF-8 in payload: {e}")))?;
        Ok(Some(content))
    }

    /// Writes a framed JSON-RPC message to a writer.
    ///
    /// # Arguments
    /// * `writer` - The writer to write to.
    /// * `payload` - The JSON-RPC message string to send.
    ///
    /// # Errors
    /// Returns [`HclError::Io`] if writing fails.
    pub fn write_message(writer: &mut dyn Write, payload: &str) -> Result<(), HclError> {
        let cr = char::from(13);
        let lf = char::from(10);
        let message = format!("Content-Length: {}{cr}{lf}{cr}{lf}{payload}", payload.len());
        writer
            .write_all(message.as_bytes())
            .map_err(|e| HclError::Io(e.to_string()))?;
        writer.flush().map_err(|e| HclError::Io(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_transport_roundtrip() -> Result<(), HclError> {
        let payload = r#"{"jsonrpc":"2.0","method":"test"}"#;
        let mut buffer = Vec::new();
        Transport::write_message(&mut buffer, payload)?;
        let mut cursor = Cursor::new(buffer);
        let read = Transport::read_message(&mut cursor)?;
        assert_eq!(read.as_deref(), Some(payload));
        let eof = Transport::read_message(&mut cursor)?;
        assert!(eof.is_none());
        Ok(())
    }

    #[test]
    fn test_transport_missing_content_length() {
        let raw = "Content-Type: application/json

{}";
        let mut cursor = Cursor::new(raw.as_bytes());
        let err = Transport::read_message(&mut cursor);
        assert!(matches!(err, Err(HclError::LspProtocol(_))));
    }

    #[test]
    fn test_transport_invalid_content_length() {
        let raw = "Content-Length: invalid

{}";
        let mut cursor = Cursor::new(raw.as_bytes());
        let err = Transport::read_message(&mut cursor);
        assert!(matches!(err, Err(HclError::LspProtocol(_))));
    }

    #[test]
    fn test_transport_error_paths() {
        struct FailingBufRead;
        impl std::io::Read for FailingBufRead {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("read fail"))
            }
        }
        impl BufRead for FailingBufRead {
            fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
                Err(std::io::Error::other("fill_buf fail"))
            }
            fn consume(&mut self, _amt: usize) {}
        }

        struct FailingWrite;
        impl Write for FailingWrite {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("write fail"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        struct FlushFailWrite;
        impl Write for FlushFailWrite {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::other("flush fail"))
            }
        }

        let eof_in_headers = "Content-Length: 10\r\n";
        let mut c1 = Cursor::new(eof_in_headers.as_bytes());
        assert!(matches!(
            Transport::read_message(&mut c1),
            Err(HclError::LspProtocol(_))
        ));
        let truncated = "Content-Length: 50\r\n\r\nshort";
        let mut c2 = Cursor::new(truncated.as_bytes());
        assert!(matches!(
            Transport::read_message(&mut c2),
            Err(HclError::Io(_))
        ));
        let invalid_utf8 = b"Content-Length: 2\r\n\r\n\xff\xfe";
        let mut c3 = Cursor::new(&invalid_utf8[..]);
        assert!(matches!(
            Transport::read_message(&mut c3),
            Err(HclError::LspProtocol(_))
        ));

        let mut failing_reader = FailingBufRead;
        assert!(std::io::Read::read(&mut failing_reader, &mut []).is_err());
        failing_reader.consume(0);
        assert!(matches!(
            Transport::read_message(&mut failing_reader),
            Err(HclError::Io(_))
        ));

        let mut failing_writer = FailingWrite;
        assert!(failing_writer.flush().is_ok());
        assert!(matches!(
            Transport::write_message(&mut failing_writer, "{}"),
            Err(HclError::Io(_))
        ));

        let mut flush_fail_writer = FlushFailWrite;
        assert!(matches!(
            Transport::write_message(&mut flush_fail_writer, "{}"),
            Err(HclError::Io(_))
        ));
    }
}
