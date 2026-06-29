//! Validation of input frontmatter against the canonical `frontmatter-schema.yaml`.
//!
//! The schema file is the single source of truth for the frontmatter rules, so
//! we embed it at build time and validate every input's frontmatter against it.
//! This keeps the compiler from re-encoding the field rules in Rust — it just
//! checks conformance.

use std::sync::OnceLock;

use anyhow::{anyhow, bail, Result};
use jsonschema::Validator;
use serde_json::Value;

/// The canonical schema, embedded from the repository root at build time.
const SCHEMA_YAML: &str = include_str!("../../frontmatter-schema.yaml");

/// Lazily-compiled validator, shared across all inputs.
fn validator() -> Result<&'static Validator> {
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();
    if let Some(v) = VALIDATOR.get() {
        return Ok(v);
    }
    let schema: Value = serde_yml::from_str(SCHEMA_YAML)
        .map_err(|e| anyhow!("parsing frontmatter-schema.yaml: {e}"))?;
    let compiled = jsonschema::validator_for(&schema)
        .map_err(|e| anyhow!("compiling frontmatter schema: {e}"))?;
    // OnceLock::set fails only if another thread won the race; either way a
    // value is present afterwards.
    let _ = VALIDATOR.set(compiled);
    Ok(VALIDATOR.get().expect("validator just set"))
}

/// Validate a parsed frontmatter document against the schema, returning an error
/// listing every violation.
pub fn validate(frontmatter: &Value) -> Result<()> {
    let validator = validator()?;
    let errors: Vec<String> = validator
        .iter_errors(frontmatter)
        .map(|e| format!("  - {} (at {})", e, e.instance_path))
        .collect();
    if !errors.is_empty() {
        bail!(
            "frontmatter does not satisfy the schema:\n{}",
            errors.join("\n")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate;
    use serde_json::json;

    #[test]
    fn accepts_a_well_formed_document() {
        let doc = json!({
            "name": "effective_power",
            "description": "why",
            "tags": ["utf8"],
            "format": "utf8",
            "decode-as": "utf8",
            "unicode-meta": {
                "code-units": 4,
                "code-points": 1,
                "scalar-values": 1,
                "legacy-grapheme-clusters": 1,
                "extended-grapheme-clusters": 1,
            },
        });
        assert!(validate(&doc).is_ok());
    }

    #[test]
    fn rejects_unicode_decode_as_without_unicode_meta() {
        let doc = json!({
            "name": "missing_unicode_meta",
            "description": "why",
            "tags": ["utf8"],
            "format": "utf8",
            "decode-as": "utf8",
        });
        assert!(validate(&doc).is_err());
    }

    #[test]
    fn non_unicode_decode_as_does_not_require_unicode_meta() {
        let doc = json!({
            "name": "raw_bytes",
            "description": "why",
            "tags": ["bytes"],
            "format": "hex",
            "decode-as": "bytes",
        });
        assert!(validate(&doc).is_ok());
    }

    #[test]
    fn rejects_unicode_meta_missing_a_count() {
        let doc = json!({
            "name": "partial_unicode_meta",
            "description": "why",
            "tags": ["utf8"],
            "format": "utf8",
            "decode-as": "utf8",
            "unicode-meta": {
                "code-units": 4,
                "code-points": 1,
                "scalar-values": 1,
                "legacy-grapheme-clusters": 1,
            },
        });
        assert!(validate(&doc).is_err());
    }

    #[test]
    fn rejects_missing_required_field() {
        let doc = json!({
            "name": "no_format",
            "description": "why",
            "tags": ["utf8"],
            "decode-as": "utf8",
        });
        assert!(validate(&doc).is_err());
    }

    #[test]
    fn rejects_file_source_without_file_format() {
        let doc = json!({
            "name": "stray_source",
            "description": "why",
            "tags": ["x"],
            "format": "utf8",
            "decode-as": "utf8",
            "file-source": "blob.bin",
        });
        assert!(validate(&doc).is_err());
    }
}
