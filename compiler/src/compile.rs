//! Compiling a single input file into validated metadata + decoded bytes, and
//! discovering input files in the corpus.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;
use walkdir::WalkDir;

use crate::format;
use crate::frontmatter;
use crate::metadata::Metadata;
use crate::schema;

/// A fully-compiled input: its frontmatter metadata plus the decoded bytes.
pub struct CompiledInput {
    pub metadata: Metadata,
    pub bytes: Vec<u8>,
}

/// Parse, validate and decode a single `.input` file.
pub fn compile(input_path: &Path) -> Result<CompiledInput> {
    compile_inner(input_path, true)
}

/// Parse and decode a single `.input` file *without* enforcing the frontmatter
/// schema. The typed [`Metadata`] is still deserialized (so its required fields
/// and types are checked), but optional, schema-conditional blocks may be absent.
///
/// This exists for tooling that needs to inspect inputs the schema would reject —
/// notably `verify-utf8`, which derives the `unicode-meta` block and so must be
/// able to read an input that is missing it.
pub fn compile_unvalidated(input_path: &Path) -> Result<CompiledInput> {
    compile_inner(input_path, false)
}

fn compile_inner(input_path: &Path, validate: bool) -> Result<CompiledInput> {
    let contents = std::fs::read_to_string(input_path)
        .with_context(|| format!("reading input file {}", input_path.display()))?;

    let split =
        frontmatter::split(&contents).with_context(|| format!("in {}", input_path.display()))?;

    // Parse the frontmatter once into a JSON value: optionally validate it against
    // the schema, then deserialize the typed metadata from the same value.
    let value: Value = serde_yml::from_str(split.frontmatter)
        .with_context(|| format!("parsing frontmatter YAML in {}", input_path.display()))?;
    if validate {
        schema::validate(&value).with_context(|| format!("in {}", input_path.display()))?;
    }
    let metadata: Metadata = serde_json::from_value(value)
        .with_context(|| format!("reading frontmatter fields in {}", input_path.display()))?;

    let bytes = format::decode(&metadata, split.data, input_path)
        .with_context(|| format!("decoding data in {}", input_path.display()))?;

    Ok(CompiledInput { metadata, bytes })
}

/// Find every `*.input` file under `inputs_dir`, sorted by path for determinism.
pub fn discover(inputs_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(inputs_dir).sort_by_file_name() {
        let entry = entry.with_context(|| format!("walking {}", inputs_dir.display()))?;
        let path = entry.path();
        if entry.file_type().is_file() && path.extension().is_some_and(|e| e == "input") {
            paths.push(path.to_path_buf());
        }
    }
    Ok(paths)
}
