//! Library for parsing, validating and compiling the trickydata input corpus.
//!
//! The corpus is a tree of `.input` files, each a YAML frontmatter document and
//! a data region (see the repository README). This crate turns those files into
//! [`CompiledInput`]s — validated [`Metadata`] plus the decoded bytes — which
//! the binaries (e.g. `compile`) assemble into distributable artifacts.

pub mod compile;
pub mod corpus;
pub mod format;
pub mod frontmatter;
pub mod hex;
pub mod index;
pub mod metadata;
pub mod schema;

pub use compile::{compile, compile_unvalidated, discover, CompiledInput};
pub use index::{build, Built, Index, IndexEntry};
pub use metadata::{Format, Metadata, Pair, PairHint};
// `corpus::Format` (the on-disk corpus format) is intentionally not re-exported
// here to avoid colliding with the input-encoding `metadata::Format` above.
pub use corpus::{load, resolve, Corpus, Resolved};
