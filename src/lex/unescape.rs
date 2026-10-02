//! String unescaping utilities for HCL string literals and heredocs.

use crate::error::HclError;
use crate::span::Span;

/// Unescape an HCL string literal or heredoc.
///
/// Supports \n, \r, \t, \", \\, \uXXXX, and \UXXXXXXXX.
///
/// # Errors
/// Returns an [`HclError::Lex`] if a unicode escape is malformed.
pub fn unescape_string(input: &str, span: &Span) -> Result<String, HclError> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next_c)) = chars.next() {
                match next_c {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'u' | 'U' => {
                        let expected_len = if next_c == 'u' { 4 } else { 8 };
                        let mut val: u32 = 0;
                        let mut read = 0;
                        let mut hex_str = String::with_capacity(expected_len);
                        while read < expected_len {
                            if let Some((_, hc)) = chars.peek()
                                && let Some(digit) = hc.to_digit(16)
                            {
                                hex_str.push(*hc);
                                val = (val << 4) | digit;
                                chars.next();
                                read += 1;
                                continue;
                            }
                            break;
                        }
                        if read == expected_len
                            && let Some(uc) = char::from_u32(val)
                        {
                            out.push(uc);
                            continue;
                        }
                        return Err(HclError::Lex(format!(
                            "Invalid unicode escape: \\{next_c}{hex_str} at {span}"
                        )));
                    }
                    _ => {
                        out.push('\\');
                        out.push(next_c);
                    }
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }

    Ok(out)
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
    fn test_unescape_standard() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        assert_eq!(unescape_string("a\\nb", &span).unwrap(), "a\nb");
        assert_eq!(unescape_string("a\\rb", &span).unwrap(), "a\rb");
        assert_eq!(unescape_string("a\\tb", &span).unwrap(), "a\tb");
        assert_eq!(unescape_string("a\\\"b", &span).unwrap(), "a\"b");
        assert_eq!(unescape_string("a\\\\b", &span).unwrap(), "a\\b");
        assert!(unescape_string("a\\ub", &span).is_err());
        assert!(unescape_string("a\\Ub", &span).is_err());
        assert_eq!(unescape_string("a\\xb", &span).unwrap(), "a\\xb");
        assert_eq!(unescape_string("a\\", &span).unwrap(), "a\\");
    }

    #[test]
    fn test_unescape_unicode() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        assert_eq!(unescape_string("\\u00E9", &span).unwrap(), "é");
        assert_eq!(unescape_string("\\U0001F600", &span).unwrap(), "😀");
        assert!(unescape_string("\\u", &span).is_err());
        assert!(unescape_string("\\U", &span).is_err());
        assert!(unescape_string("\\u00E", &span).is_err());
        assert!(unescape_string("\\u00G0", &span).is_err());
        assert!(unescape_string("\\U0001F60", &span).is_err());
        assert!(unescape_string("\\uD800", &span).is_err());
        assert!(unescape_string("\\UFFFFFFFF", &span).is_err());
    }
}
