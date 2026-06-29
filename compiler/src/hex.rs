//! Parser for the `hex` input mini-language.
//!
//! The grammar (see README.md) is intentionally small:
//!
//! * Pairs of hex digits encode one byte each; letters are case-insensitive.
//! * Whitespace between tokens is optional and ignored. Whitespace is any
//!   character with the Unicode `White_Space=True` property (`char::is_whitespace`).
//! * `#` starts a comment that runs to the end of the line.
//! * `"..."` introduces a quoted run of ASCII characters (anything except `"`),
//!   whose bytes are emitted verbatim.
//!
//! Hex digits are paired across whitespace/comments, so `0a0b`, `0a 0b` and
//! `0a # x\n0b` all yield the same two bytes. An odd number of hex digits is an
//! error.

use anyhow::{bail, Result};

/// Parse a `hex`-format data region into its raw bytes.
pub fn decode(input: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    // Holds the first nibble of a byte while we wait for its partner.
    let mut pending: Option<u8> = None;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // Comment: skip to end of line.
            '#' => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
            }
            // Quoted ASCII string: emit bytes verbatim until the closing quote.
            '"' => {
                if pending.is_some() {
                    bail!("quoted string starts mid-byte (dangling hex digit before '\"')");
                }
                let mut closed = false;
                for n in chars.by_ref() {
                    if n == '"' {
                        closed = true;
                        break;
                    }
                    if !n.is_ascii() {
                        bail!("non-ASCII character {n:?} in quoted hex string");
                    }
                    out.push(n as u8);
                }
                if !closed {
                    bail!("unterminated quoted string in hex input");
                }
            }
            // Hex digit: accumulate into the current byte.
            c if c.is_ascii_hexdigit() => {
                let nibble = c.to_digit(16).unwrap() as u8;
                match pending.take() {
                    None => pending = Some(nibble),
                    Some(hi) => out.push((hi << 4) | nibble),
                }
            }
            // Whitespace separates tokens and is otherwise ignored.
            c if c.is_whitespace() => {}
            // Anything else is invalid.
            other => bail!("unexpected character {other:?} in hex input"),
        }
    }

    if pending.is_some() {
        bail!("hex input has an odd number of hex digits");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn pairs_with_optional_whitespace() {
        assert_eq!(decode("00 11 22").unwrap(), vec![0x00, 0x11, 0x22]);
        assert_eq!(decode("001122").unwrap(), vec![0x00, 0x11, 0x22]);
        assert_eq!(decode("aA bB").unwrap(), vec![0xaa, 0xbb]);
    }

    #[test]
    fn comments_are_ignored() {
        let bytes = decode("03 00 # width\n05 00 # height\n").unwrap();
        assert_eq!(bytes, vec![0x03, 0x00, 0x05, 0x00]);
    }

    #[test]
    fn quoted_strings_emit_ascii_bytes() {
        let bytes = decode("\"GIF89a\" # magic\n03 00").unwrap();
        let mut expected = b"GIF89a".to_vec();
        expected.extend_from_slice(&[0x03, 0x00]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn hex_digits_pair_across_whitespace_and_comments() {
        assert_eq!(decode("0a # x\n0b").unwrap(), vec![0x0a, 0x0b]);
    }

    #[test]
    fn odd_digit_count_is_an_error() {
        assert!(decode("0a0").is_err());
    }

    #[test]
    fn unterminated_quote_is_an_error() {
        assert!(decode("\"oops").is_err());
    }
}
