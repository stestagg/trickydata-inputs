//! `make-index` — compile the input corpus into `trickydata.bin` and a
//! pretty-printed `trickydata-index.json`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Serialize;
use trickydata_compiler::{compile, discover, Metadata, Pair, PairHint};

const BIN_NAME: &str = "trickydata.bin";
const INDEX_NAME: &str = "trickydata-index.json";

/// Compile the trickydata input corpus into `trickydata.bin` and
/// `trickydata-index.json`.
#[derive(Parser)]
#[command(name = "make-index", about)]
struct Args {
    /// Directory to write trickydata.bin and trickydata-index.json into.
    #[arg(default_value = ".")]
    output_dir: PathBuf,

    /// Directory to scan recursively for `*.input` files.
    #[arg(long, default_value = "inputs")]
    inputs: PathBuf,

    /// Version string recorded at the top of the generated index.
    #[arg(long, default_value = "dev")]
    version: String,
}

/// One input's metadata plus its source path and location within `trickydata.bin`.
#[derive(Serialize)]
struct IndexEntry {
    #[serde(flatten)]
    metadata: Metadata,
    /// Source `.input` file path, relative to the working directory.
    path: String,
    offset: usize,
    length: usize,
}

/// The top-level index document.
#[derive(Serialize)]
struct Index {
    /// Version of this compiled corpus (defaults to "dev").
    version: String,
    /// Name of the companion binary blob the offsets/lengths index into.
    bin: String,
    inputs: Vec<IndexEntry>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !args.inputs.is_dir() {
        bail!(
            "inputs directory '{}' not found; pass --inputs <dir> or run from the repository root",
            args.inputs.display()
        );
    }

    // Compile every input, keeping its source path alongside.
    let mut compiled = Vec::new();
    for path in discover(&args.inputs)? {
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
            path: path.to_string_lossy().into_owned(),
            offset,
            length,
        });
    }
    let count = inputs.len();

    let index = Index {
        version: args.version,
        bin: BIN_NAME.to_string(),
        inputs,
    };

    // Write artifacts.
    std::fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("creating output directory {}", args.output_dir.display()))?;

    let bin_path = args.output_dir.join(BIN_NAME);
    std::fs::write(&bin_path, &blob).with_context(|| format!("writing {}", bin_path.display()))?;

    let index_path = args.output_dir.join(INDEX_NAME);
    let mut json = serde_json::to_string_pretty(&index).context("serializing index")?;
    json.push('\n');
    std::fs::write(&index_path, json)
        .with_context(|| format!("writing {}", index_path.display()))?;

    println!(
        "Compiled {count} input(s): {} bytes -> {} and {}",
        blob.len(),
        bin_path.display(),
        index_path.display()
    );
    Ok(())
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
