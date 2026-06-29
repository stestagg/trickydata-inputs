//! Turning an input's data region into raw bytes, according to its `format`.
//!
//! This is the compiler's one real job beyond schema validation: it is naive to
//! `decode-as`/`mime-type` and only cares about how the bytes are *encoded* in
//! the corpus, not how a client will later interpret them.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::hex;
use crate::metadata::{Format, Metadata};

/// Decode the data region of an input file into its raw bytes.
///
/// * `data` is the text after the `\n---\n` separator (empty/whitespace for the
///   `file` format).
/// * `input_path` is the path to the `.input` file, used to resolve the
///   companion file for the `file` format.
pub fn decode(meta: &Metadata, data: &str, input_path: &Path) -> Result<Vec<u8>> {
    match meta.format {
        Format::Utf8 => Ok(data.as_bytes().to_vec()),
        Format::Utf8Strip => Ok(strip_whitespace(data).as_bytes().to_vec()),
        Format::Hex => hex::decode(data),
        Format::File => decode_file(meta, data, input_path),
    }
}

/// Trim leading and trailing whitespace, i.e. any character with the Unicode
/// `White_Space=True` derived property (which is exactly `char::is_whitespace`).
fn strip_whitespace(s: &str) -> &str {
    s.trim_matches(char::is_whitespace)
}

/// Read the companion file for a `file`-format input.
///
/// The source path is `file-source` (relative to the input file's directory)
/// when given, otherwise the input path with its `.input` extension stripped.
/// Any non-whitespace in the in-file data region is an error.
fn decode_file(meta: &Metadata, data: &str, input_path: &Path) -> Result<Vec<u8>> {
    if !data.trim().is_empty() {
        bail!("`file` format input has inline data after the frontmatter; data must live in the companion file only");
    }

    let dir = input_path.parent().unwrap_or_else(|| Path::new("."));
    let source = match &meta.file_source {
        Some(rel) => dir.join(rel),
        None => input_path.with_extension(""),
    };

    std::fs::read(&source).with_context(|| format!("reading companion file {}", source.display()))
}

#[cfg(test)]
mod tests {
    use super::strip_whitespace;

    #[test]
    fn strips_leading_and_trailing_whitespace() {
        // Mix of ASCII spaces/newlines/tabs and U+2003 EM SPACE (all White_Space);
        // interior whitespace is preserved.
        assert_eq!(strip_whitespace("\u{2003}\n\t a\nb \t\u{2003}"), "a\nb");
    }

    #[test]
    fn strips_newlines_and_tabs_at_the_edges() {
        // Newlines and tabs have White_Space=True, so they are stripped too.
        assert_eq!(strip_whitespace("\n\t a \t\n"), "a");
    }

    #[test]
    fn preserves_non_whitespace_only_strings() {
        assert_eq!(strip_whitespace("abc"), "abc");
    }
}
