//! Corpus formats: the interchangeable on-disk shapes of a compiled corpus.
//!
//! A [`Built`] (index document + binary blob) can be read from / written to
//! several formats, each a [`Corpus`] bound to its on-disk location:
//!
//! - [`SourceCorpus`] — the `.input` source tree (a directory).
//! - [`TrickydataCorpus`] — a single `<prefix>.trickydata` file (preferred).
//! - [`ZipCorpus`] — a single `<prefix>.zip` holding the json-bin members.
//! - [`JsonBinCorpus`] — the legacy `<prefix>-index.json` + `<prefix>.bin` pair.
//!
//! The source tree is just another format: it reads via the compiler and writes
//! by re-emitting `.input` files. Because some source encodings are normalising
//! (`utf8-strip` discards edge whitespace, `file` data lives in a companion
//! file), a re-emitted source tree is not byte-identical to the original, but it
//! recompiles to the same [`Built`]. So all formats are equivalent at the parsed
//! (index + blob) level, which is how [`resolve`] and the round-trip tests treat
//! them.
//!
//! [`resolve`] maps a user-given path to the right corpus: an existing file
//! (format by extension), a directory (the source tree), the best existing file
//! for a stem, or — when nothing exists — a corpus derived from the path.

use std::ffi::OsString;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use crate::index::{build, Built, Index};
use crate::metadata::{Format as InputFormat, Metadata};

/// Default filename stem the artifacts are written under and compared by.
pub const DEFAULT_PREFIX: &str = "trickydata";
/// Suffix appended to an artifact prefix to name the JSON index.
pub const INDEX_SUFFIX: &str = "-index.json";
/// Suffix appended to an artifact prefix to name the binary blob.
pub const BIN_SUFFIX: &str = ".bin";
/// Default name of the companion blob recorded in an index's `bin` field
/// (equals [`DEFAULT_PREFIX`] + [`BIN_SUFFIX`]).
pub const BIN_NAME: &str = "trickydata.bin";

/// Magic prefix every `.trickydata` file opens with.
const MAGIC: &[u8] = b"trickydata:";
/// Suffix naming a single-file `.trickydata` artifact.
const TRICKYDATA_SUFFIX: &str = ".trickydata";
/// Suffix naming a single-file `.zip` artifact.
const ZIP_SUFFIX: &str = ".zip";
/// Version recorded when reading a source tree (which does not store one).
const SOURCE_VERSION: &str = "dev";

/// The corpus formats. `CONTAINERS` are the distributable single-prefix formats,
/// listed in preference order (the one [`resolve`] picks first for a stem);
/// `Source` is the `.input` tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum Format {
    Source,
    Trickydata,
    Zip,
    JsonBin,
}

impl Format {
    /// Every format, source first.
    pub const ALL: [Format; 4] = [
        Format::Source,
        Format::Trickydata,
        Format::Zip,
        Format::JsonBin,
    ];

    /// The distributable single-prefix container formats, in preference order.
    pub const CONTAINERS: [Format; 3] = [Format::Trickydata, Format::Zip, Format::JsonBin];

    /// Build this format's corpus bound to `location` — a stem for the container
    /// formats (e.g. `./trickydata` -> `./trickydata.trickydata`) or the root
    /// directory for the source tree.
    pub fn corpus_for(self, location: &Path) -> Box<dyn Corpus> {
        match self {
            Format::Source => Box::new(SourceCorpus::new(location)),
            Format::Trickydata => Box::new(TrickydataCorpus(append(location, TRICKYDATA_SUFFIX))),
            Format::Zip => Box::new(ZipCorpus(append(location, ZIP_SUFFIX))),
            Format::JsonBin => Box::new(JsonBinCorpus {
                index: append(location, INDEX_SUFFIX),
                bin: append(location, BIN_SUFFIX),
            }),
        }
    }
}

/// A compiled corpus in one format, bound to its on-disk location.
pub trait Corpus {
    /// Which format this corpus reads/writes.
    fn format(&self) -> Format;
    /// The path(s) this corpus reads/writes — a directory for the source tree.
    fn paths(&self) -> Vec<PathBuf>;
    /// Write `built` to disk in this format.
    fn write(&self, built: &Built) -> Result<()>;
    /// Read this corpus back into memory.
    fn read(&self) -> Result<Built>;
}

/// The `.input` source tree, rooted at a directory.
pub struct SourceCorpus {
    pub root: PathBuf,
    pub version: String,
}

impl SourceCorpus {
    /// A source corpus rooted at `root`, recording the default version on read.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            version: SOURCE_VERSION.to_string(),
        }
    }
}

impl Corpus for SourceCorpus {
    fn format(&self) -> Format {
        Format::Source
    }

    fn paths(&self) -> Vec<PathBuf> {
        vec![self.root.clone()]
    }

    fn write(&self, built: &Built) -> Result<()> {
        // Entry paths are root-relative, so re-rooting is just a join under
        // `self.root`; the relative structure that `pair` links resolve against
        // is preserved.
        for entry in &built.index.inputs {
            let path = self.root.join(&entry.path);
            let bytes = built
                .blob
                .get(entry.offset..entry.offset + entry.length)
                .ok_or_else(|| anyhow!("entry '{}' is out of blob bounds", entry.metadata.name))?;
            emit_input(&path, &entry.metadata, bytes)?;
        }
        Ok(())
    }

    fn read(&self) -> Result<Built> {
        build(&self.root, &self.version)
            .with_context(|| format!("compiling source corpus in {}", self.root.display()))
    }
}

/// The legacy pair: a pretty-printed JSON index plus a raw binary blob.
pub struct JsonBinCorpus {
    pub index: PathBuf,
    pub bin: PathBuf,
}

impl Corpus for JsonBinCorpus {
    fn format(&self) -> Format {
        Format::JsonBin
    }

    fn paths(&self) -> Vec<PathBuf> {
        vec![self.index.clone(), self.bin.clone()]
    }

    fn write(&self, built: &Built) -> Result<()> {
        fs::write(&self.bin, &built.blob)
            .with_context(|| format!("writing {}", self.bin.display()))?;
        // Record the sidecar's name in the index so a json-bin loader can find
        // the blob beside it. This is the one format that ships a separate file.
        let bin_name = self
            .bin
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(BIN_NAME)
            .to_string();
        fs::write(&self.index, index_json_with_bin(&built.index, Some(bin_name))?)
            .with_context(|| format!("writing {}", self.index.display()))?;
        Ok(())
    }

    fn read(&self) -> Result<Built> {
        let json = fs::read(&self.index)
            .with_context(|| format!("reading index {}", self.index.display()))?;
        let index = parse_index(&json)
            .with_context(|| format!("parsing index {}", self.index.display()))?;
        let blob = fs::read(&self.bin)
            .with_context(|| format!("reading blob {}", self.bin.display()))?;
        Ok(Built { index, blob })
    }
}

/// The single-file `.trickydata` format (see module + README for the layout).
pub struct TrickydataCorpus(pub PathBuf);

impl Corpus for TrickydataCorpus {
    fn format(&self) -> Format {
        Format::Trickydata
    }

    fn paths(&self) -> Vec<PathBuf> {
        vec![self.0.clone()]
    }

    fn write(&self, built: &Built) -> Result<()> {
        let bytes = encode_trickydata(built)?;
        fs::write(&self.0, bytes).with_context(|| format!("writing {}", self.0.display()))
    }

    fn read(&self) -> Result<Built> {
        let bytes = fs::read(&self.0).with_context(|| format!("reading {}", self.0.display()))?;
        decode_trickydata(&bytes).with_context(|| format!("parsing {}", self.0.display()))
    }
}

/// A single `.zip` holding the json-bin members.
pub struct ZipCorpus(pub PathBuf);

impl Corpus for ZipCorpus {
    fn format(&self) -> Format {
        Format::Zip
    }

    fn paths(&self) -> Vec<PathBuf> {
        vec![self.0.clone()]
    }

    fn write(&self, built: &Built) -> Result<()> {
        let bytes = encode_zip(built)?;
        fs::write(&self.0, bytes).with_context(|| format!("writing {}", self.0.display()))
    }

    fn read(&self) -> Result<Built> {
        let bytes = fs::read(&self.0).with_context(|| format!("reading {}", self.0.display()))?;
        decode_zip(&bytes).with_context(|| format!("parsing {}", self.0.display()))
    }
}

/// Outcome of [`resolve`]: how a user-given path maps onto a corpus.
pub enum Resolved {
    /// `path` is an existing file or directory, in a single known format.
    ExactMatch(Box<dyn Corpus>),
    /// `path` is a stem; one or more container formats exist, in preference order.
    StemMatch(Vec<Box<dyn Corpus>>),
    /// Nothing on disk. `Some` if a format could be derived from `path`/`expected`.
    NoMatch(Option<Box<dyn Corpus>>),
}

/// Resolve a user-given `path` to a corpus.
///
/// 1. An existing file is an [`Resolved::ExactMatch`]: format from `expected`, or
///    inferred from the extension (an unknown, unexpected file yields
///    `NoMatch(None)`).
/// 2. An existing directory is the source tree, also [`Resolved::ExactMatch`].
/// 3. Otherwise probe the stem over the container formats in preference order,
///    collecting those whose file(s) all exist, as [`Resolved::StemMatch`].
/// 4. Otherwise [`Resolved::NoMatch`], carrying a corpus derived from the path's
///    extension or `expected` when one is known.
///
/// `expected`, when set, restricts every step to that single format.
pub fn resolve(path: &Path, expected: Option<Format>) -> Resolved {
    if path.is_file() {
        return match expected.or_else(|| format_from_path(path)) {
            Some(format) => Resolved::ExactMatch(corpus_from_file(path, format)),
            None => Resolved::NoMatch(None),
        };
    }

    if path.is_dir() && expected.is_none_or(|e| e == Format::Source) {
        return Resolved::ExactMatch(Box::new(SourceCorpus::new(path)));
    }

    let found: Vec<Box<dyn Corpus>> = Format::CONTAINERS
        .iter()
        .copied()
        .filter(|f| expected.is_none_or(|e| e == *f))
        .map(|f| f.corpus_for(path))
        .filter(|c| c.paths().iter().all(|p| p.is_file()))
        .collect();
    if !found.is_empty() {
        return Resolved::StemMatch(found);
    }

    if let Some(format) = format_from_path(path) {
        if expected.is_none_or(|e| e == format) {
            return Resolved::NoMatch(Some(corpus_from_file(path, format)));
        }
    }
    match expected {
        Some(format) => Resolved::NoMatch(Some(format.corpus_for(path))),
        None => Resolved::NoMatch(None),
    }
}

/// [`resolve`] `path` and read the corpus it points at, erroring when nothing
/// matches. For a stem with several container formats present, the preferred one
/// is read.
pub fn load(path: &Path, expected: Option<Format>) -> Result<Built> {
    match resolve(path, expected) {
        Resolved::ExactMatch(corpus) => corpus.read(),
        Resolved::StemMatch(mut corpora) => corpora.remove(0).read(),
        Resolved::NoMatch(_) => bail!(
            "no corpus found at '{}' (expected a source directory or a compiled artifact)",
            path.display()
        ),
    }
}

/// Map a concrete file path to its container format by extension.
fn format_from_path(path: &Path) -> Option<Format> {
    let name = path.file_name()?.to_str()?;
    if name.ends_with(TRICKYDATA_SUFFIX) {
        Some(Format::Trickydata)
    } else if name.ends_with(ZIP_SUFFIX) {
        Some(Format::Zip)
    } else if name.ends_with(INDEX_SUFFIX) || name.ends_with(BIN_SUFFIX) {
        Some(Format::JsonBin)
    } else {
        None
    }
}

/// Build a corpus bound to a concrete `path` for a known `format`. For json-bin
/// the companion member is derived from the given member's stem.
fn corpus_from_file(path: &Path, format: Format) -> Box<dyn Corpus> {
    match format {
        Format::Source => Box::new(SourceCorpus::new(path)),
        Format::Trickydata => Box::new(TrickydataCorpus(path.to_path_buf())),
        Format::Zip => Box::new(ZipCorpus(path.to_path_buf())),
        Format::JsonBin => Format::JsonBin.corpus_for(&jsonbin_prefix(path)),
    }
}

/// Strip a json-bin member suffix to recover the artifact prefix.
fn jsonbin_prefix(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    for suffix in [INDEX_SUFFIX, BIN_SUFFIX] {
        if let Some(stem) = s.strip_suffix(suffix) {
            return PathBuf::from(stem.to_string());
        }
    }
    path.to_path_buf()
}

/// Append a literal suffix to a path's final component.
fn append(prefix: &Path, suffix: &str) -> PathBuf {
    let mut name = prefix.as_os_str().to_os_string();
    name.push(OsString::from(suffix));
    PathBuf::from(name)
}

// --- Source emission -------------------------------------------------------

/// Re-emit a single `.input` file (and, for `file` inputs, its companion data
/// file) from one index entry's metadata and bytes.
fn emit_input(path: &Path, meta: &Metadata, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut frontmatter = serde_yml::to_string(meta).context("serializing frontmatter")?;
    while frontmatter.ends_with('\n') {
        frontmatter.pop();
    }
    let data = encode_data(meta, bytes, path)?;
    fs::write(path, format!("{frontmatter}\n---\n{data}"))
        .with_context(|| format!("writing {}", path.display()))
}

/// Encode an entry's bytes into the data region for its `format`, writing the
/// companion file (and returning an empty region) for `file` inputs.
fn encode_data(meta: &Metadata, bytes: &[u8], input_path: &Path) -> Result<String> {
    match meta.format {
        // utf8-strip stores already-stripped bytes, so re-stripping on read is a
        // no-op: writing them verbatim round-trips.
        InputFormat::Utf8 | InputFormat::Utf8Strip => String::from_utf8(bytes.to_vec())
            .context("input bytes are not valid UTF-8 and cannot be written as text"),
        InputFormat::Hex => Ok(hex_encode(bytes)),
        InputFormat::File => {
            let companion = input_path.with_extension("");
            fs::write(&companion, bytes)
                .with_context(|| format!("writing companion file {}", companion.display()))?;
            Ok(String::new())
        }
    }
}

/// Render bytes as space-separated lowercase hex (re-parsed by [`crate::hex`]).
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3 + 1);
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out.push('\n');
    out
}

// --- JSON index serialization ----------------------------------------------

/// Serialize the index document the way all formats embed it: pretty-printed
/// with a trailing newline.
fn index_json(index: &Index) -> Result<String> {
    let mut json = serde_json::to_string_pretty(index).context("serializing index")?;
    json.push('\n');
    Ok(json)
}

/// Serialize the index with its `bin` locator set to what the calling format
/// ships: `Some(name)` for formats with a real sidecar/member, `None` for the
/// single-file `.trickydata`. Avoids cloning when the value already matches
/// (e.g. a freshly `build`-ed index whose `bin` is already `None`).
fn index_json_with_bin(index: &Index, bin: Option<String>) -> Result<String> {
    if index.bin == bin {
        index_json(index)
    } else {
        index_json(&Index {
            bin,
            ..index.clone()
        })
    }
}

/// Parse the index document from its serialized JSON bytes.
fn parse_index(json: &[u8]) -> Result<Index> {
    serde_json::from_slice(json).context("parsing index")
}

// --- .trickydata codec -----------------------------------------------------

/// Encode a corpus into the `.trickydata` byte layout.
fn encode_trickydata(built: &Built) -> Result<Vec<u8>> {
    // The blob is embedded inline, so there is no sidecar to name.
    let gz = gzip(index_json_with_bin(&built.index, None)?.as_bytes())?;
    let digest = md5::compute(&gz);
    let len = u32::try_from(gz.len()).context("compressed index too large for u32 length")?;

    let mut out = Vec::with_capacity(gz.len() + built.blob.len() + 64);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(built.index.version.as_bytes());
    out.push(0);
    out.extend_from_slice(&digest.0);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&gz);
    out.extend_from_slice(&digest.0); // repeated, as a trailing bookend
    out.push(0);
    out.extend_from_slice(&built.blob);
    Ok(out)
}

/// Decode the `.trickydata` byte layout, verifying both checksums.
fn decode_trickydata(bytes: &[u8]) -> Result<Built> {
    let mut p = bytes
        .strip_prefix(MAGIC)
        .ok_or_else(|| anyhow!("missing 'trickydata:' magic prefix"))?;

    let zero = p
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| anyhow!("missing version terminator"))?;
    let version = std::str::from_utf8(&p[..zero])
        .context("version string is not valid UTF-8")?
        .to_string();
    p = &p[zero + 1..];

    let leading = take(&mut p, 16, "leading checksum")?;
    let len = u32::from_le_bytes(take(&mut p, 4, "index length")?.try_into().unwrap()) as usize;
    let gz = take(&mut p, len, "compressed index")?;
    if md5::compute(gz).0 != leading {
        bail!("index checksum mismatch (corrupt .trickydata)");
    }
    let trailing = take(&mut p, 16, "trailing checksum")?;
    if trailing != leading {
        bail!("trailing index checksum mismatch (corrupt .trickydata)");
    }
    if take(&mut p, 1, "blob separator")?[0] != 0 {
        bail!("expected zero separator before blob");
    }

    let index = parse_index(&gunzip(gz)?)?;
    if index.version != version {
        bail!(
            "header version '{}' does not match index version '{}'",
            version,
            index.version
        );
    }
    Ok(Built {
        index,
        blob: p.to_vec(),
    })
}

/// Split `n` bytes off the front of `p`, erroring if the input is truncated.
fn take<'a>(p: &mut &'a [u8], n: usize, what: &str) -> Result<&'a [u8]> {
    if p.len() < n {
        bail!("truncated .trickydata: expected {n} more byte(s) for {what}");
    }
    let (head, tail) = p.split_at(n);
    *p = tail;
    Ok(head)
}

/// gzip `data` with a zeroed mtime so the output is byte-for-byte reproducible.
fn gzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::default());
    enc.write_all(data).context("compressing index")?;
    enc.finish().context("finishing index compression")
}

/// Inflate gzip `data`.
fn gunzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(data)
        .read_to_end(&mut out)
        .context("decompressing index")?;
    Ok(out)
}

// --- zip codec -------------------------------------------------------------

/// Member names for the zip — the default json-bin member names.
fn zip_member_names() -> (String, String) {
    (
        format!("{DEFAULT_PREFIX}{INDEX_SUFFIX}"),
        BIN_NAME.to_string(),
    )
}

/// Encode a corpus into a deterministic zip of the json-bin members.
fn encode_zip(built: &Built) -> Result<Vec<u8>> {
    let (index_name, bin_name) = zip_member_names();
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());

    let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zw.start_file(index_name, options)
        .context("starting zip index member")?;
    // The zip ships the blob as a real member, so its index names it like
    // json-bin does.
    zw.write_all(index_json_with_bin(&built.index, Some(bin_name.clone()))?.as_bytes())
        .context("writing zip index member")?;
    zw.start_file(bin_name, options)
        .context("starting zip blob member")?;
    zw.write_all(&built.blob).context("writing zip blob member")?;
    Ok(zw.finish().context("finishing zip")?.into_inner())
}

/// Decode a zip back into a corpus, matching members by suffix.
fn decode_zip(bytes: &[u8]) -> Result<Built> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("opening zip")?;
    let mut index_bytes: Option<Vec<u8>> = None;
    let mut blob: Option<Vec<u8>> = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("reading zip member")?;
        let name = entry.name().to_string();
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .with_context(|| format!("reading zip member {name}"))?;
        if name.ends_with(INDEX_SUFFIX) {
            index_bytes = Some(data);
        } else if name.ends_with(BIN_SUFFIX) {
            blob = Some(data);
        }
    }
    let index =
        parse_index(&index_bytes.ok_or_else(|| anyhow!("zip is missing an index member"))?)?;
    let blob = blob.ok_or_else(|| anyhow!("zip is missing a .bin member"))?;
    Ok(Built { index, blob })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::IndexEntry;

    /// A small, schema-valid corpus: one `hex` entry so the source reader (which
    /// validates frontmatter) accepts it on round-trip.
    fn sample() -> Built {
        let metadata = Metadata {
            name: "alpha".to_string(),
            description: "an entry".to_string(),
            tags: vec!["t".to_string()],
            format: InputFormat::Hex,
            decode_as: Some("bytes".to_string()),
            mime_type: None,
            invalid_as: None,
            licence: None,
            pair: None,
            unicode_meta: None,
            file_source: None,
        };
        let blob = vec![0x00, 0x11, 0xff, 0x2a];
        let index = Index {
            version: SOURCE_VERSION.to_string(),
            bin: None,
            inputs: vec![IndexEntry {
                metadata,
                path: "inputs/alpha.input".to_string(),
                offset: 0,
                length: blob.len(),
            }],
        };
        Built { index, blob }
    }

    /// A unique scratch directory for a test.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-corpus-{tag}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Built compared at the parsed level: blob bytes plus the index document
    /// with the json-bin-only `bin` locator removed (it is a format presentation
    /// detail, not corpus content). Entry `path`s are root-relative, so they are
    /// compared as-is and must match across roots and formats.
    fn normalized(built: &Built) -> (Vec<u8>, serde_json::Value) {
        let mut value = serde_json::to_value(&built.index).unwrap();
        value.as_object_mut().unwrap().remove("bin");
        (built.blob.clone(), value)
    }

    #[test]
    fn containers_round_trip() {
        let dir = scratch("containers");
        let prefix = dir.join("trickydata");
        let built = sample();
        for format in Format::CONTAINERS {
            let corpus = format.corpus_for(&prefix);
            corpus.write(&built).unwrap();
            let back = corpus.read().unwrap();
            assert_eq!(normalized(&back), normalized(&built));
        }
    }

    #[test]
    fn source_tree_round_trips_and_reads_as_built() {
        let dir = scratch("source");
        // Write the sample as a source tree, then read it back as the reference.
        let src = SourceCorpus::new(&dir);
        src.write(&sample()).unwrap();
        let reference = src.read().unwrap();
        // It really materialised .input files at their root-relative paths.
        assert!(dir.join("inputs/alpha.input").is_file());
        assert_eq!(normalized(&reference), normalized(&sample()));
    }

    #[test]
    fn every_format_round_trips_back_to_source() {
        let dir = scratch("cross");
        let reference = {
            let src = SourceCorpus::new(dir.join("orig"));
            src.write(&sample()).unwrap();
            src.read().unwrap()
        };

        // Each container preserves the corpus, and re-emitting a source tree from
        // it recompiles to the same parsed corpus.
        for format in Format::CONTAINERS {
            let prefix = dir.join(format!("{format:?}"));
            let container = format.corpus_for(&prefix);
            container.write(&reference).unwrap();
            let via_container = container.read().unwrap();
            assert_eq!(normalized(&via_container), normalized(&reference));

            let src = SourceCorpus::new(dir.join(format!("{format:?}-src")));
            src.write(&via_container).unwrap();
            let back_to_source = src.read().unwrap();
            assert_eq!(normalized(&back_to_source), normalized(&reference));
        }
    }

    #[test]
    fn real_corpus_round_trips_through_source() {
        // Exercises the source writer over every real input format (utf8,
        // utf8-strip, hex, file) and pair links. Skipped when the corpus is not
        // checked out alongside the crate.
        let inputs = Path::new("../inputs");
        if !inputs.is_dir() {
            return;
        }
        let reference = SourceCorpus::new(inputs).read().unwrap();

        let dir = scratch("real");
        let src = SourceCorpus::new(&dir);
        src.write(&reference).unwrap();
        let back = src.read().unwrap();

        assert_eq!(normalized(&back), normalized(&reference));
    }

    #[test]
    fn trickydata_rejects_corruption() {
        let built = sample();
        let mut bytes = encode_trickydata(&built).unwrap();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xff;
        assert!(decode_trickydata(&bytes).is_err());
        assert!(decode_trickydata(b"not-a-trickydata-file").is_err());
    }

    #[test]
    fn resolve_picks_preferred_dir_and_missing() {
        let dir = scratch("resolve");
        let prefix = dir.join("trickydata");
        let built = sample();

        assert!(matches!(resolve(&prefix, None), Resolved::NoMatch(_)));

        // A directory resolves to the source tree.
        match resolve(&dir, None) {
            Resolved::ExactMatch(c) => assert_eq!(c.format(), Format::Source),
            other => panic!("expected source ExactMatch, got {:?}", Dbg(&other)),
        }

        // Two containers present -> the preferred existing one (zip over json-bin).
        Format::Zip.corpus_for(&prefix).write(&built).unwrap();
        Format::JsonBin.corpus_for(&prefix).write(&built).unwrap();
        match resolve(&prefix, None) {
            Resolved::StemMatch(v) => assert_eq!(v[0].format(), Format::Zip),
            other => panic!("expected StemMatch, got {:?}", Dbg(&other)),
        }

        // `expected` filters to json-bin even though zip exists.
        match resolve(&prefix, Some(Format::JsonBin)) {
            Resolved::StemMatch(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].format(), Format::JsonBin);
            }
            other => panic!("expected filtered StemMatch, got {:?}", Dbg(&other)),
        }

        // An exact existing file infers its format.
        match resolve(&append(&prefix, ZIP_SUFFIX), None) {
            Resolved::ExactMatch(c) => assert_eq!(c.format(), Format::Zip),
            other => panic!("expected ExactMatch, got {:?}", Dbg(&other)),
        }
    }

    /// Printable view of `Resolved` for test panics (it holds trait objects).
    struct Dbg<'a>(&'a Resolved);
    impl std::fmt::Debug for Dbg<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self.0 {
                Resolved::ExactMatch(_) => write!(f, "ExactMatch"),
                Resolved::StemMatch(v) => write!(f, "StemMatch({})", v.len()),
                Resolved::NoMatch(_) => write!(f, "NoMatch"),
            }
        }
    }
}
