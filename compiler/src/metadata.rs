//! The frontmatter metadata of an input file.
//!
//! These structs mirror the fields described in `frontmatter-schema.yaml`. Only
//! `format` is given a meaningful type here, because it is the one field the
//! compiler must act on to turn an input into bytes. Everything else
//! (`decode-as`, `mime-type`, `licence`) is opaque passthrough metadata that
//! rides along to downstream clients untouched.

use serde::{Deserialize, Serialize};

/// How the data portion of an input file is encoded. This is the only
/// frontmatter field the compiler interprets, since it selects the byte decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Utf8,
    Utf8Strip,
    Hex,
    File,
}

/// How two paired inputs relate, recorded on each [`Pair`] link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairHint {
    /// The two inputs look different but should compare equal.
    Equal,
    /// The two inputs look the same but should compare unequal.
    NotEqual,
    /// Together the two inputs form a tricky case.
    Tricky,
}

/// A link to another input that is meaningful to use alongside this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair {
    /// Relative path, from the linking input file, to the other input.
    pub with: String,
    /// How the two inputs relate.
    pub hint: PairHint,
}

/// Counts of the Unicode segmentation units in decoded text. Required for
/// inputs decoded as a Unicode encoding (utf8, utf16, utf32) so downstream
/// clients can validate their own segmentation against known-good totals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnicodeMeta {
    #[serde(rename = "code-units")]
    pub code_units: u64,

    #[serde(rename = "code-points")]
    pub code_points: u64,

    #[serde(rename = "scalar-values")]
    pub scalar_values: u64,

    #[serde(rename = "legacy-grapheme-clusters")]
    pub legacy_grapheme_clusters: u64,

    #[serde(rename = "extended-grapheme-clusters")]
    pub extended_grapheme_clusters: u64,
}

/// The parsed frontmatter of an input file.
///
/// Field renames keep the JSON/YAML keys in the project's kebab-case style.
/// Optional fields are skipped when serializing if absent, so the generated
/// index stays compact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub format: Format,

    #[serde(rename = "decode-as", default, skip_serializing_if = "Option::is_none")]
    pub decode_as: Option<String>,

    #[serde(rename = "mime-type", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    /// The type this input would decode as if it were valid. Set on
    /// deliberately-invalid inputs, which are stored as `decode-as: bytes`.
    #[serde(rename = "invalid-as", default, skip_serializing_if = "Option::is_none")]
    pub invalid_as: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,

    /// Other inputs that pair meaningfully with this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair: Option<Vec<Pair>>,

    /// Unicode segmentation counts; required for Unicode `decode-as` values.
    #[serde(rename = "unicode-meta", default, skip_serializing_if = "Option::is_none")]
    pub unicode_meta: Option<UnicodeMeta>,

    /// Where `file`-format data lives, relative to the input file. This is a
    /// compile-time source detail, so it is always skipped when serializing and
    /// never leaks into the generated index.
    #[serde(rename = "file-source", default, skip_serializing)]
    pub file_source: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::BTreeSet;

    /// `frontmatter-schema.yaml` and `Metadata` are two hand-written views of the
    /// same data. This test fails the build if they drift: every property named
    /// in the schema must map to a field here, and vice-versa.
    ///
    /// When this fails, add the field to *both* the struct and the schema (or
    /// remove it from both).
    #[test]
    fn schema_properties_and_struct_fields_stay_in_sync() {
        // Populate every optional field so all keys serialize.
        let meta = Metadata {
            name: String::new(),
            description: String::new(),
            tags: Vec::new(),
            format: Format::Utf8,
            decode_as: Some(String::new()),
            mime_type: Some(String::new()),
            invalid_as: Some(String::new()),
            licence: Some(String::new()),
            pair: Some(Vec::new()),
            unicode_meta: Some(UnicodeMeta {
                code_units: 0,
                code_points: 0,
                scalar_values: 0,
                legacy_grapheme_clusters: 0,
                extended_grapheme_clusters: 0,
            }),
            file_source: Some(String::new()),
        };
        let Value::Object(map) = serde_json::to_value(&meta).unwrap() else {
            unreachable!("Metadata serializes to a JSON object");
        };
        let mut struct_fields: BTreeSet<String> = map.into_iter().map(|(k, _)| k).collect();
        // `file-source` is intentionally skip_serializing, so add it back here.
        struct_fields.insert("file-source".to_string());

        let schema: Value =
            serde_yml::from_str(include_str!("../../frontmatter-schema.yaml")).unwrap();
        let schema_fields: BTreeSet<String> = schema["properties"]
            .as_object()
            .expect("schema has a properties map")
            .keys()
            .cloned()
            .collect();

        assert_eq!(
            struct_fields, schema_fields,
            "Metadata fields and frontmatter-schema.yaml properties have drifted"
        );
    }
}
