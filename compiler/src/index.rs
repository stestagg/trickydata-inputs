//! Assembling compiled inputs into the corpus index document + binary blob.
//!
//! This is the in-memory data model and the [`build`] that produces it from the
//! input corpus. How that [`Built`] is laid out on disk — and read back — lives
//! in [`crate::corpus`], so the binaries share one source of truth for the index
//! regardless of which container format they read or write.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::{compile, discover, Metadata, Pair, PairHint};

/// One input's metadata plus its source path and location within the blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    #[serde(flatten)]
    pub metadata: Metadata,
    /// Source `.input` file path, relative to the inputs root (forward slashes).
    pub path: String,
    pub offset: usize,
    pub length: usize,
}

/// The top-level index document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    /// Version of this compiled corpus (defaults to "dev").
    pub version: String,
    /// Name of the companion binary blob, recorded only by the json-bin format
    /// (where the blob is a separate sidecar file). The single-file formats embed
    /// the blob, so they leave this unset and it is omitted from their index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
    pub inputs: Vec<IndexEntry>,
}

/// A compiled corpus held in memory: the index document and the blob its
/// entries index into.
pub struct Built {
    pub index: Index,
    pub blob: Vec<u8>,
}

/// Compile every `*.input` file under `inputs_dir` into an in-memory corpus:
/// the index document plus the binary blob it references.
pub fn build(inputs_dir: &Path, version: &str) -> Result<Built> {
    // Compile every input, keeping its source path alongside.
    let mut compiled = Vec::new();
    for path in discover(inputs_dir)? {
        let input = compile(&path)?;
        compiled.push((path, input));
    }

    // Deterministic order keyed on the input's unique name.
    compiled.sort_by(|(_, a), (_, b)| a.metadata.name.cmp(&b.metadata.name));

    // Index inputs by name (rejecting duplicates) and by normalized source path,
    // so `pair` links — which reference the other input by relative path — can be
    // resolved to a known input.
    let mut names: HashSet<&str> = HashSet::new();
    let mut name_at_path: HashMap<PathBuf, &str> = HashMap::new();
    for (path, input) in &compiled {
        let name = input.metadata.name.as_str();
        if !names.insert(name) {
            bail!("duplicate input name '{}'", input.metadata.name);
        }
        name_at_path.insert(normalize(path), name);
    }

    // Resolve and validate every `pair` link, then make it symmetric: a declared
    // `A -> B` link implies `B -> A` as well, carrying the same hint, so both
    // entries list each other in the index regardless of which one declared it.
    // Each link's `with` is recorded as a canonical relative path from the listing
    // input file; the BTreeMap dedups by that path and keeps a deterministic order.
    let mut links: HashMap<&str, BTreeMap<String, PairHint>> = HashMap::new();
    for (path, input) in &compiled {
        let dir = normalize(path.parent().unwrap_or_else(|| Path::new("")));
        let source_path = normalize(path);
        for pair in input.metadata.pair.iter().flatten() {
            let target_path = normalize(&dir.join(&pair.with));
            let Some(&target) = name_at_path.get(&target_path) else {
                bail!(
                    "input '{}' ({}) has a pair link with: '{}', which resolves to '{}' (no such input)",
                    input.metadata.name,
                    path.display(),
                    pair.with,
                    target_path.display()
                );
            };
            if target == input.metadata.name {
                bail!(
                    "input '{}' ({}) pairs with itself",
                    input.metadata.name,
                    path.display()
                );
            }
            // Forward link, as a canonical relative path from this input.
            links
                .entry(input.metadata.name.as_str())
                .or_default()
                .insert(path_string(&relative_path(&dir, &target_path)), pair.hint);
            // Symmetric reverse link, relative to the target input's directory.
            let target_dir = target_path.parent().unwrap_or_else(|| Path::new(""));
            links
                .entry(target)
                .or_default()
                .insert(path_string(&relative_path(target_dir, &source_path)), pair.hint);
        }
    }

    // Assemble the blob and index, substituting the symmetric link set.
    let mut blob: Vec<u8> = Vec::new();
    let mut inputs: Vec<IndexEntry> = Vec::new();
    for (path, input) in &compiled {
        let offset = blob.len();
        let length = input.bytes.len();
        blob.extend_from_slice(&input.bytes);

        let mut metadata = input.metadata.clone();
        metadata.pair = links.get(input.metadata.name.as_str()).map(|set| {
            set.iter()
                .map(|(with, hint)| Pair {
                    with: with.clone(),
                    hint: *hint,
                })
                .collect()
        });

        inputs.push(IndexEntry {
            metadata,
            // Recorded relative to the inputs root (with forward slashes) so the
            // index is independent of where the corpus was compiled from.
            path: path_string(path.strip_prefix(inputs_dir).unwrap_or(path.as_path())),
            offset,
            length,
        });
    }

    let index = Index {
        version: version.to_string(),
        // Format-agnostic: the json-bin writer stamps the sidecar name on write.
        bin: None,
        inputs,
    };

    Ok(Built { index, blob })
}

/// Lexically normalize a path, resolving `.` and `..` without touching the
/// filesystem (input paths are repo-relative and need not exist as written).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Build a relative path from directory `from_dir` to file `to`, both assumed
/// already normalized. Used to express a pair link as a path relative to the
/// listing input file.
fn relative_path(from_dir: &Path, to: &Path) -> PathBuf {
    let from: Vec<Component> = from_dir.components().collect();
    let to: Vec<Component> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut rel = PathBuf::new();
    for _ in common..from.len() {
        rel.push("..");
    }
    for comp in &to[common..] {
        rel.push(comp.as_os_str());
    }
    rel
}

/// Render a path with forward slashes so generated links are stable regardless
/// of the host platform's path separator.
fn path_string(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
