//! JSON-RPC 2.0 transport layer with LSP header framing.
//!
//! Handles `Content-Length` message framing over any standard
//! [`Read`](std::io::Read) and [`Write`] streams, supporting both stdio and TCP.

use crate::error::LspError;
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
    /// Returns [`LspError::Protocol`] or [`LspError::Io`] if headers are malformed or reading fails.
    pub fn read_message(reader: &mut dyn BufRead) -> Result<Option<String>, LspError> {
        let mut content_length: Option<usize> = None;

        loop {
            let mut line = String::new();
            let bytes_read = reader
                .read_line(&mut line)
                .map_err(|e| LspError::Io(e.to_string()))?;

            if bytes_read == 0 {
                if content_length.is_none() {
                    return Ok(None);
                }
                return Err(LspError::Protocol(
                    "Unexpected EOF while reading headers".to_string(),
                ));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                // Header section complete
                break;
            }

            if let Some(stripped) = trimmed.strip_prefix("Content-Length:") {
                let len_str = stripped.trim();
                let len = len_str.parse::<usize>().map_err(|_| {
                    LspError::Protocol(format!("Invalid Content-Length value: {len_str}"))
                })?;
                content_length = Some(len);
            }
        }

        let Some(len) = content_length else {
            return Err(LspError::Protocol(
                "Missing Content-Length header".to_string(),
            ));
        };

        let mut buffer = vec![0u8; len];
        reader
            .read_exact(&mut buffer)
            .map_err(|e| LspError::Io(format!("Failed to read message body: {e}")))?;

        let content = String::from_utf8(buffer)
            .map_err(|e| LspError::Protocol(format!("Invalid UTF-8 in payload: {e}")))?;

        Ok(Some(content))
    }

    /// Writes a framed JSON-RPC message to a writer.
    ///
    /// # Arguments
    /// * `writer` - The writer to write to.
    /// * `payload` - The JSON-RPC message string to send.
    ///
    /// # Errors
    /// Returns [`LspError::Io`] if writing fails.
    pub fn write_message(writer: &mut dyn Write, payload: &str) -> Result<(), LspError> {
        let message = format!("Content-Length: {}\r\n\r\n{payload}", payload.len());
        writer
            .write_all(message.as_bytes())
            .map_err(|e| LspError::Io(e.to_string()))?;
        writer.flush().map_err(|e| LspError::Io(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_transport_roundtrip() {
        let payload = r#"{"jsonrpc":"2.0","method":"test"}"#;
        let mut buffer = Vec::new();
        Transport::write_message(&mut buffer, payload).expect("write ok");

        let mut cursor = Cursor::new(buffer);
        let read = Transport::read_message(&mut cursor)
            .expect("read ok")
            .expect("has message");
        assert_eq!(read, payload);

        // EOF clean
        let eof = Transport::read_message(&mut cursor).expect("clean eof");
        assert!(eof.is_none());
    }

    #[test]
    fn test_transport_missing_content_length() {
        let raw = "Content-Type: application/json\r\n\r\n{}";
        let mut cursor = Cursor::new(raw.as_bytes());
        let err = Transport::read_message(&mut cursor);
        assert!(err.is_err());
    }

    #[test]
    fn test_transport_invalid_content_length() {
        let raw = "Content-Length: invalid\r\n\r\n{}";
        let mut cursor = Cursor::new(raw.as_bytes());
        let err = Transport::read_message(&mut cursor);
        assert!(err.is_err());
    }

    #[test]
    fn test_transport_error_paths() {
        // 1. Unexpected EOF in headers after content-length
        let eof_in_headers = "Content-Length: 10\r\n";
        let mut c1 = Cursor::new(eof_in_headers.as_bytes());
        assert!(Transport::read_message(&mut c1).is_err());

        // 2. Failed to read full message body (truncated payload)
        let truncated = "Content-Length: 50\r\n\r\nshort";
        let mut c2 = Cursor::new(truncated.as_bytes());
        assert!(Transport::read_message(&mut c2).is_err());

        // 3. Invalid UTF-8 in payload
        let invalid_utf8 = b"Content-Length: 2\r\n\r\n\xff\xfe";
        let mut c3 = Cursor::new(&invalid_utf8[..]);
        assert!(Transport::read_message(&mut c3).is_err());

        // 4. Failing reader
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
        let mut failing_reader = FailingBufRead;
        assert!(std::io::Read::read(&mut failing_reader, &mut []).is_err());
        failing_reader.consume(0);
        assert!(Transport::read_message(&mut failing_reader).is_err());

        // 5. Failing writer
        struct FailingWrite;
        impl Write for FailingWrite {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("write fail"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut failing_writer = FailingWrite;
        assert!(failing_writer.flush().is_ok());
        assert!(Transport::write_message(&mut failing_writer, "{}").is_err());

        // 6. Failing flush
        struct FlushFailWrite;
        impl Write for FlushFailWrite {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::other("flush fail"))
            }
        }
        let mut flush_fail_writer = FlushFailWrite;
        assert!(Transport::write_message(&mut flush_fail_writer, "{}").is_err());
    }
}
