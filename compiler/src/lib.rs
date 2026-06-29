//! Library for parsing, validating and compiling the trickydata input corpus.
//!
//! The corpus is a tree of `.input` files, each a YAML frontmatter document and
//! a data region (see the repository README). This crate turns those files into
//! [`CompiledInput`]s — validated [`Metadata`] plus the decoded bytes — which
//! the binaries (e.g. `make-index`) assemble into distributable artifacts.

pub mod compile;
pub mod format;
pub mod frontmatter;
pub mod hex;
pub mod metadata;
pub mod schema;

pub use compile::{compile, compile_unvalidated, discover, CompiledInput};
pub use metadata::{Format, Metadata, Pair, PairHint};
