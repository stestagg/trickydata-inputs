//! `compile` — compile (or convert) the input corpus into distributable
//! artifacts.
//!
//! It reads the corpus from `--inputs` in whatever format is found there (a
//! `.input` source tree by default, or any compiled artifact — see
//! `--inputs-format`) and writes it back out. By default it writes all three
//! container formats for the `trickydata` prefix: `trickydata.trickydata`
//! (preferred), `trickydata.zip`, and the legacy `trickydata-index.json` +
//! `trickydata.bin` pair. Individual formats can be turned off with `--no-...`,
//! or a single one selected with `--format` (including `--format source` to
//! re-emit a normalised `.input` tree, which is never written by default).

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use trickydata_compiler::corpus::{load, Format, DEFAULT_PREFIX};

/// Compile or convert the trickydata input corpus into its distributable
/// artifacts.
#[derive(Parser)]
#[command(name = "compile", about)]
struct Args {
    /// Directory to write the compiled artifacts into.
    #[arg(default_value = ".")]
    output_dir: PathBuf,

    /// Input corpus to read: a `.input` source directory (the default) or a
    /// compiled artifact / prefix in any format.
    #[arg(long, default_value = "inputs")]
    inputs: PathBuf,

    /// Format to read `--inputs` as. Inferred from what is found when omitted;
    /// set it to disambiguate a prefix that has several formats present.
    #[arg(long, value_enum)]
    inputs_format: Option<Format>,

    /// Version recorded in the generated index. Overrides the version read from
    /// the input; defaults to the input's own (`dev` for a source tree).
    #[arg(long)]
    version: Option<String>,

    /// Write only this single format instead of all containers. Conflicts with
    /// the matching `--no-...` flag. `source` re-emits a `.input` tree into the
    /// output directory.
    #[arg(long, value_enum)]
    format: Option<Format>,

    /// Skip writing the single-file `.trickydata` format.
    #[arg(long)]
    no_trickydata: bool,

    /// Skip writing the `.zip` format.
    #[arg(long)]
    no_zip: bool,

    /// Skip writing the legacy `-index.json` + `.bin` pair.
    #[arg(long)]
    no_json_bin: bool,
}

impl Args {
    /// Whether a given container format is disabled by its `--no-...` flag.
    fn disabled(&self, format: Format) -> bool {
        match format {
            Format::Trickydata => self.no_trickydata,
            Format::Zip => self.no_zip,
            Format::JsonBin => self.no_json_bin,
            Format::Source => false,
        }
    }

    /// The formats to write. `--format` selects exactly one (any format, source
    /// included); otherwise every container not disabled by a `--no-...` flag.
    fn enabled_formats(&self) -> Result<Vec<Format>> {
        if let Some(only) = self.format {
            if self.disabled(only) {
                bail!("--format {only:?} conflicts with its own --no-... flag");
            }
            return Ok(vec![only]);
        }
        let formats: Vec<Format> = Format::CONTAINERS
            .into_iter()
            .filter(|f| !self.disabled(*f))
            .collect();
        if formats.is_empty() {
            bail!("all output formats are disabled; enable at least one");
        }
        Ok(formats)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let enabled = args.enabled_formats()?;

    let mut built = load(&args.inputs, args.inputs_format)
        .with_context(|| format!("reading inputs from {}", args.inputs.display()))?;
    if let Some(version) = &args.version {
        built.index.version = version.clone();
    }
    let count = built.index.inputs.len();

    std::fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("creating output directory {}", args.output_dir.display()))?;

    let prefix = args.output_dir.join(DEFAULT_PREFIX);

    let mut written = Vec::new();
    for format in enabled {
        // Container formats are written as `<output_dir>/trickydata.<ext>`; the
        // source tree is re-emitted directly under the output directory.
        let location = match format {
            Format::Source => args.output_dir.clone(),
            _ => prefix.clone(),
        };
        let corpus = format.corpus_for(&location);
        corpus.write(&built)?;
        written.extend(corpus.paths());
    }

    let written: Vec<String> = written.iter().map(|p| p.display().to_string()).collect();
    println!(
        "Compiled {count} input(s): {} bytes -> {}",
        built.blob.len(),
        written.join(", ")
    );
    Ok(())
}
