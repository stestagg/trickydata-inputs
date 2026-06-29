//! `verify-utf8` — recompute the `unicode-meta` block for every UTF-8 input and
//! check it against the value recorded in the frontmatter.
//!
//! For each input whose `decode-as` is `utf8`, the decoded bytes are read back as
//! a UTF-8 string and the five segmentation counts (`code-units`, `code-points`,
//! `scalar-values`, `legacy-grapheme-clusters`, `extended-grapheme-clusters`) are
//! derived from scratch. A file whose recorded block matches every derived count
//! prints a green tick; any disagreement prints a red cross. Every mismatch is
//! then listed, sorted by path then field name, with a fully generic justification
//! of the expected value (its character decomposition and raw byte array) so the
//! discrepancy can be analysed without reference to the specific input.
//!
//! Inputs are compiled *without* schema validation, so an input that omits its
//! `unicode-meta` block entirely is still checked: every field reports an actual
//! value of `<MISSING>`, and the derived counts in the report can be copied
//! straight into the new input. This lets an author add a UTF-8 input and have the
//! correct block computed for them.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use trickydata_compiler::{compile_unvalidated, discover, Metadata};
use unicode_segmentation::UnicodeSegmentation;

/// The `decode-as` value whose `unicode-meta` this command knows how to derive.
/// (`utf8` is the only Unicode encoding the corpus currently stores; utf16/utf32
/// would each need their own code-unit definition.)
const DECODE_AS_UTF8: &str = "utf8";

/// Placeholder shown for a field whose value the frontmatter does not record
/// (i.e. the whole `unicode-meta` block is absent).
const MISSING: &str = "<MISSING>";

/// Recompute and verify the `unicode-meta` block of every `decode-as: utf8` input.
#[derive(Parser)]
#[command(name = "verify-utf8", about)]
struct Args {
    /// Directory to scan recursively for `*.input` files.
    #[arg(long, default_value = "inputs")]
    inputs: PathBuf,
}

/// One field of `unicode-meta`, with the value we derived and the value the
/// frontmatter recorded (`None` when the whole block is absent).
struct FieldCheck {
    /// The frontmatter key, e.g. `code-points`.
    name: &'static str,
    /// The value derived from the decoded text.
    expected: u64,
    /// The value recorded in the frontmatter, if any.
    actual: Option<u64>,
    /// A self-contained justification of `expected`.
    description: String,
}

impl FieldCheck {
    fn matches(&self) -> bool {
        self.actual == Some(self.expected)
    }
}

/// A single field disagreement, carried to the end-of-run report.
struct Mismatch {
    path: String,
    field: &'static str,
    expected: u64,
    /// Rendered actual value (a number, or `(absent)` when the block is missing).
    actual: String,
    description: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Returns `Ok(true)` when every UTF-8 input verified, `Ok(false)` on any mismatch.
fn run() -> Result<bool> {
    let args = Args::parse();
    let color = std::io::stdout().is_terminal();

    let mut mismatches: Vec<Mismatch> = Vec::new();

    for path in discover(&args.inputs)? {
        // Compile without schema validation: an input that is missing its
        // `unicode-meta` block (e.g. a freshly-authored one) would fail the schema,
        // but we want to read it anyway and report the values it should contain.
        let input = compile_unvalidated(&path)
            .with_context(|| format!("compiling {}", path.display()))?;
        if input.metadata.decode_as.as_deref() != Some(DECODE_AS_UTF8) {
            continue;
        }
        let path_str = path.to_string_lossy().into_owned();

        // decode-as: utf8 promises the decoded bytes are well-formed UTF-8. If the
        // promise is broken we cannot derive any count, so report that directly.
        let text = match std::str::from_utf8(&input.bytes) {
            Ok(text) => text,
            Err(err) => {
                print_line(&path_str, false, color);
                mismatches.push(Mismatch {
                    path: path_str,
                    field: "decode-as",
                    expected: 0,
                    actual: "(invalid)".to_string(),
                    description: format!(
                        "decode-as is utf8 but the decoded bytes are not valid UTF-8: {err}. \
                         Bytes: {}.",
                        byte_array(&input.bytes)
                    ),
                });
                continue;
            }
        };

        let checks = field_checks(text, &input.metadata);
        let ok = checks.iter().all(FieldCheck::matches);
        print_line(&path_str, ok, color);

        for check in checks {
            if !check.matches() {
                mismatches.push(Mismatch {
                    path: path_str.clone(),
                    field: check.name,
                    expected: check.expected,
                    actual: check
                        .actual
                        .map_or_else(|| MISSING.to_string(), |v| v.to_string()),
                    description: check.description,
                });
            }
        }
    }

    if mismatches.is_empty() {
        return Ok(true);
    }

    // Report every mismatch, ordered by path then field name.
    mismatches.sort_by(|a, b| a.path.cmp(&b.path).then(a.field.cmp(b.field)));
    println!();
    for m in &mismatches {
        println!(
            "{}: Mismatch: {} Expected: {} Actual: {}. {}",
            m.path, m.field, m.expected, m.actual, m.description
        );
    }
    Ok(false)
}

/// Build the five field checks for one UTF-8 input from its decoded text.
fn field_checks(text: &str, meta: &Metadata) -> Vec<FieldCheck> {
    let um = meta.unicode_meta.as_ref();

    let code_units = text.len() as u64;
    let code_points = text.chars().count() as u64;
    // A `&str` can only hold scalar values (UTF-8 cannot encode surrogates), so
    // every code point is a scalar value and the two counts coincide.
    let scalar_values = code_points;
    let legacy = text.graphemes(false).count() as u64;
    let extended = text.graphemes(true).count() as u64;

    vec![
        FieldCheck {
            name: "code-units",
            expected: code_units,
            actual: um.map(|u| u.code_units),
            description: format!(
                "UTF-8 code units are bytes; the decoded data is {code_units} byte(s): {}.",
                byte_array(text.as_bytes())
            ),
        },
        FieldCheck {
            name: "code-points",
            expected: code_points,
            actual: um.map(|u| u.code_points),
            description: format!(
                "{code_points} Unicode code point(s): {}.",
                code_point_list(text)
            ),
        },
        FieldCheck {
            name: "scalar-values",
            expected: scalar_values,
            actual: um.map(|u| u.scalar_values),
            description: format!(
                "Well-formed UTF-8 encodes only scalar values (no surrogates), so scalar \
                 values equal code points: {scalar_values}. Code points: {}.",
                code_point_list(text)
            ),
        },
        FieldCheck {
            name: "legacy-grapheme-clusters",
            expected: legacy,
            actual: um.map(|u| u.legacy_grapheme_clusters),
            description: format!(
                "{legacy} legacy grapheme cluster(s) (UAX #29 legacy boundaries): {}.",
                cluster_list(text, false)
            ),
        },
        FieldCheck {
            name: "extended-grapheme-clusters",
            expected: extended,
            actual: um.map(|u| u.extended_grapheme_clusters),
            description: format!(
                "{extended} extended grapheme cluster(s) (UAX #29 extended boundaries): {}.",
                cluster_list(text, true)
            ),
        },
    ]
}

/// Print one `✓`/`✗` status line for a file, green/red when stdout is a terminal.
fn print_line(path: &str, ok: bool, color: bool) {
    let (mark, code) = if ok { ('✓', "32") } else { ('✗', "31") };
    if color {
        println!("\x1b[{code}m{mark} {path}\x1b[0m");
    } else {
        println!("{mark} {path}");
    }
}

/// Render bytes as a hex array, e.g. `[0xEF, 0xBF, 0xBF]`.
fn byte_array(bytes: &[u8]) -> String {
    let body = bytes
        .iter()
        .map(|b| format!("0x{b:02X}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{body}]")
}

/// Render the text's code points as `U+XXXX [hex bytes]` tokens, in order, so the
/// code-point and scalar-value counts can be checked character by character.
fn code_point_list(text: &str) -> String {
    text.chars().map(code_point_token).collect::<Vec<_>>().join(", ")
}

/// One code point as `U+XXXX [0xNN, …]`, pairing its scalar value with its own
/// UTF-8 encoding so the byte total can be reconciled with the code-point total.
fn code_point_token(c: char) -> String {
    let mut buf = [0u8; 4];
    let bytes = c.encode_utf8(&mut buf).as_bytes();
    format!("U+{:04X} {}", c as u32, byte_array(bytes))
}

/// Render the grapheme segmentation as bracketed clusters of `U+XXXX` code points,
/// e.g. `[U+0069 U+0307]`, so the cluster boundaries are explicit. `extended`
/// selects UAX #29 extended (`true`) vs legacy (`false`) boundaries.
fn cluster_list(text: &str, extended: bool) -> String {
    text.graphemes(extended)
        .map(|g| {
            let cps = g
                .chars()
                .map(|c| format!("U+{:04X}", c as u32))
                .collect::<Vec<_>>()
                .join(" ");
            format!("[{cps}]")
        })
        .collect::<Vec<_>>()
        .join(" ")
}
