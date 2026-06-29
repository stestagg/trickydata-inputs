//! `compare` — check two compiled corpus artifacts for equivalent entries.
//!
//! Takes one or two artifact *prefixes*: a prefix `./trickydata` names the pair
//! `./trickydata.bin` + `./trickydata-index.json`. When only one prefix is
//! given, the second side is compiled in memory from the input corpus, so this
//! doubles as an "are the committed artifacts up to date?" check.
//!
//! Comparison is by *entry*: each input is matched by name, and its metadata,
//! source path and decoded bytes are compared. The corpus `version` header is
//! deliberately ignored, since it is a release-time stamp rather than content.
//!
//! Exit status is 0 when the two sides have identical entries, and non-zero
//! (with the differences listed on stderr) when they differ.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::{Map, Value};
use trickydata_compiler::corpus::load;
use trickydata_compiler::IndexEntry;

/// Compare two compiled corpus artifacts (or one against a fresh in-memory
/// build of the input corpus) and report any differing entries.
#[derive(Parser)]
#[command(name = "compare", about)]
struct Args {
    /// Artifact prefix to check, e.g. `./trickydata` for `./trickydata.bin`
    /// and `./trickydata-index.json`.
    prefix: PathBuf,

    /// Second artifact prefix to compare against. When omitted, the corpus is
    /// compiled in memory from `--inputs` and used as the comparison side.
    other: Option<PathBuf>,

    /// Directory to scan recursively for `*.input` files when building the
    /// in-memory comparison side (only used when no second prefix is given).
    #[arg(long, default_value = "inputs")]
    inputs: PathBuf,
}

/// One side of a comparison: its entries plus the blob they index into, and a
/// human-readable label for messages.
struct Corpus {
    label: String,
    entries: Vec<IndexEntry>,
    blob: Vec<u8>,
}

impl Corpus {
    /// Load a corpus from an on-disk artifact path or prefix, in whichever
    /// format is present (the preferred one if several match a prefix).
    fn from_prefix(prefix: &Path) -> Result<Self> {
        let built = load(prefix, None)?;
        Ok(Self {
            label: prefix.display().to_string(),
            entries: built.index.inputs,
            blob: built.blob,
        })
    }

    /// Build a corpus in memory from the input corpus directory (the source tree
    /// is just another corpus format).
    fn from_inputs(inputs_dir: &Path) -> Result<Self> {
        let built = load(inputs_dir, None)
            .with_context(|| format!("compiling inputs from {}", inputs_dir.display()))?;
        Ok(Self {
            label: format!("<built from {}>", inputs_dir.display()),
            entries: built.index.inputs,
            blob: built.blob,
        })
    }

    /// Decoded bytes of one entry, sliced out of the blob.
    fn bytes<'a>(&'a self, entry: &IndexEntry) -> &'a [u8] {
        self.blob
            .get(entry.offset..entry.offset + entry.length)
            .unwrap_or(&[])
    }

    /// Index entries by their unique input name.
    fn by_name(&self) -> BTreeMap<&str, &IndexEntry> {
        self.entries
            .iter()
            .map(|e| (e.metadata.name.as_str(), e))
            .collect()
    }
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();

    let left = Corpus::from_prefix(&args.prefix)?;
    let right = match &args.other {
        Some(other) => Corpus::from_prefix(other)?,
        None => Corpus::from_inputs(&args.inputs)?,
    };

    let differences = compare(&left, &right);
    if differences.is_empty() {
        println!(
            "{} and {} have identical entries ({} input(s)).",
            left.label,
            right.label,
            left.entries.len()
        );
        return Ok(ExitCode::SUCCESS);
    }

    eprintln!(
        "{} and {} differ ({} difference(s)):",
        left.label,
        right.label,
        differences.len()
    );
    for line in &differences {
        eprintln!("  - {line}");
    }
    eprintln!(
        "\nThe committed artifacts are out of date. Recompile with `compile` \
         (or let the pre-commit hook do it) and commit the result."
    );
    Ok(ExitCode::FAILURE)
}

/// Compare two corpora entry-by-entry, returning a sorted list of human-readable
/// differences. An empty list means the entries are equivalent.
fn compare(left: &Corpus, right: &Corpus) -> Vec<String> {
    let left_by_name = left.by_name();
    let right_by_name = right.by_name();

    let names: BTreeSet<&str> = left_by_name
        .keys()
        .chain(right_by_name.keys())
        .copied()
        .collect();

    let mut diffs = Vec::new();
    for name in names {
        match (left_by_name.get(name), right_by_name.get(name)) {
            (Some(_), None) => diffs.push(format!("'{name}' only in {}", left.label)),
            (None, Some(_)) => diffs.push(format!("'{name}' only in {}", right.label)),
            (Some(l), Some(r)) => {
                for field in entry_diffs(l, left.bytes(l), r, right.bytes(r)) {
                    diffs.push(format!("'{name}': {field}"));
                }
            }
            (None, None) => unreachable!("name came from one of the two maps"),
        }
    }
    diffs
}

/// Field-level differences between two entries of the same name: metadata
/// fields, source path and decoded bytes. The corpus-relative `offset` is
/// ignored (it is positional, not content).
fn entry_diffs(l: &IndexEntry, l_bytes: &[u8], r: &IndexEntry, r_bytes: &[u8]) -> Vec<String> {
    let mut diffs = Vec::new();

    let lv = metadata_map(l);
    let rv = metadata_map(r);
    let keys: BTreeSet<&String> = lv.keys().chain(rv.keys()).collect();
    for key in keys {
        let lval = lv.get(key);
        let rval = rv.get(key);
        if lval != rval {
            diffs.push(format!(
                "{key} {} != {}",
                render(lval),
                render(rval)
            ));
        }
    }

    if l.path != r.path {
        diffs.push(format!("path {:?} != {:?}", l.path, r.path));
    }

    if l_bytes != r_bytes {
        diffs.push(format!(
            "data differs ({} vs {} bytes)",
            l_bytes.len(),
            r_bytes.len()
        ));
    }

    diffs
}

/// Serialize an entry's metadata to a JSON object for field-by-field comparison.
fn metadata_map(entry: &IndexEntry) -> Map<String, Value> {
    match serde_json::to_value(&entry.metadata) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// Render an optional JSON value compactly for a diff message.
fn render(value: Option<&Value>) -> String {
    match value {
        Some(v) => v.to_string(),
        None => "(absent)".to_string(),
    }
}
