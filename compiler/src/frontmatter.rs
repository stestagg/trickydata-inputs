//! Splitting an input file into its YAML frontmatter and data region.
//!
//! Per README.md, everything before the first `\n---\n` is the frontmatter
//! document and everything after it is the data region.

use anyhow::{bail, Result};

const SEPARATOR: &str = "\n---\n";

/// The two halves of an input file.
pub struct Split<'a> {
    /// The YAML frontmatter text (before the separator).
    pub frontmatter: &'a str,
    /// The data region (after the separator). Empty/whitespace for `file` inputs.
    pub data: &'a str,
}

/// Split an input file's contents on the first `\n---\n` separator.
pub fn split(contents: &str) -> Result<Split<'_>> {
    match contents.find(SEPARATOR) {
        Some(idx) => Ok(Split {
            frontmatter: &contents[..idx],
            data: &contents[idx + SEPARATOR.len()..],
        }),
        None => bail!("missing frontmatter separator: expected a line containing only '---'"),
    }
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn splits_on_first_separator() {
        let s = split("name: x\n---\nbody\n---\nmore").unwrap();
        assert_eq!(s.frontmatter, "name: x");
        assert_eq!(s.data, "body\n---\nmore");
    }

    #[test]
    fn missing_separator_errors() {
        assert!(split("name: x\nno separator here").is_err());
    }
}
